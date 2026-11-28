//! CDP session attachment, service worker / extension page shim injection, SW restart handling,
//! default-world & off-origin binding enforcement, bounded probe/timeout handling, and
//! deterministic Chromium `tabId` probing.

#![forbid(unsafe_code)]

use std::collections::HashSet;
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use super::manifest::{extension_id_from_url, is_valid_ext_id};
use super::{
    AttachedSession, ExecutionContextMeta, ExpectedTarget, ExtEffect, ExtensionHost, PendingRpc,
    ProbeInFlight, SessionKind, rpc, shim_source,
};

const MAX_METRIC_SAMPLES: usize = 256;
const MAX_PENDING_RPCS: usize = 256;
const MAX_BINDING_PAYLOAD_BYTES: usize = 256 * 1024;
const PROBE_TIMEOUT: Duration = Duration::from_millis(1500);

impl ExtensionHost {
    pub fn on_cdp_message(&mut self, msg: Value) -> Vec<ExtEffect> {
        let mut effects = Vec::new();
        if let Some(id) = msg.get("id").and_then(Value::as_i64) {
            return self.handle_cdp_reply(id, &msg);
        }
        let Some(method) = msg.get("method").and_then(Value::as_str) else {
            return effects;
        };
        let params = msg.get("params").cloned().unwrap_or(Value::Null);
        let session_id = msg.get("sessionId").and_then(Value::as_str).map(str::to_owned);

        match method {
            "Target.attachedToTarget" => effects.extend(self.handle_attached(&params)),
            "Target.detachedFromTarget" => {
                if let Some(sid) = params.get("sessionId").and_then(Value::as_str) {
                    self.cleanup_detached_session(sid);
                }
            }
            "Runtime.exceptionThrown" => {
                if let Some(sid) = session_id
                    && let Some(ext_id) = self
                        .sessions
                        .get(&sid)
                        .filter(|s| s.kind == SessionKind::ServiceWorker)
                        .map(|s| s.ext_id.clone())
                {
                    self.metrics.sw_uncaught_exceptions += 1;
                    if self.metrics.sw_exception_samples.len() < 16 {
                        let details = params.get("exceptionDetails");
                        let desc = details
                            .and_then(|d| d.pointer("/exception/description").or_else(|| d.get("text")))
                            .and_then(Value::as_str)
                            .unwrap_or("uncaught exception");
                        let sample: String = format!("{ext_id}: {desc}").chars().take(200).collect();
                        self.metrics.sw_exception_samples.push(sample);
                    }
                }
            }
            "Fetch.requestPaused" => {
                if let Some(sid) = session_id
                    && let Some(req_id) = params.get("requestId").and_then(Value::as_str)
                {
                    // Any request matching our `file://*` / `blob:file://*` Fetch patterns from
                    // an extension service worker, offscreen document, or extension page fails
                    // closed immediately (`review-rd274.md` §2).
                    let _ = self.send_cdp(
                        "Fetch.failRequest",
                        json!({ "requestId": req_id, "errorReason": "BlockedByClient" }),
                        Some(&sid),
                    );
                }
            }
            "Page.frameNavigated" => {
                if let Some(sid) = session_id
                    && let Some(frame) = params.get("frame")
                    && frame.get("parentId").is_none()
                    && let Some(url) = frame.get("url").and_then(Value::as_str)
                {
                    let frame_id = frame.get("id").and_then(Value::as_str).map(str::to_owned);
                    let should_detach = match self.sessions.get_mut(&sid) {
                        Some(sess) if sess.kind != SessionKind::ServiceWorker => {
                            if extension_id_from_url(url) == Some(sess.ext_id.as_str()) {
                                sess.main_frame_id = frame_id;
                                sess.url = url.to_owned();
                                false
                            } else {
                                // Main frame left `chrome-extension://<ext_id>/` (or an initially
                                // blank target turned out to be a web/isolated tab): detach NOW.
                                true
                            }
                        }
                        _ => false,
                    };
                    if should_detach {
                        self.detach_session(&sid);
                    }
                }
            }
            "Runtime.executionContextCreated" => {
                if let Some(sid) = session_id
                    && let Some(ctx) = params.get("context")
                    && let Some(ctx_id) = ctx.get("id").and_then(Value::as_i64)
                    && let Some(sess) = self.sessions.get_mut(&sid)
                {
                    let origin = ctx
                        .get("origin")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .trim_end_matches('/')
                        .to_owned();
                    let aux = ctx.get("auxData");
                    let (is_default, ctx_type, frame_id) = if sess.kind == SessionKind::ServiceWorker {
                        (true, "default".to_owned(), None)
                    } else {
                        let def = aux.and_then(|a| a.get("isDefault")).and_then(Value::as_bool).unwrap_or(false);
                        let ty = aux.and_then(|a| a.get("type")).and_then(Value::as_str).unwrap_or("").to_owned();
                        let fid = aux.and_then(|a| a.get("frameId")).and_then(Value::as_str).map(str::to_owned);
                        if def
                            && ty == "default"
                            && sess.main_frame_id.is_none()
                            && extension_id_from_url(&origin) == Some(sess.ext_id.as_str())
                        {
                            sess.main_frame_id = fid.clone();
                        }
                        (def, ty, fid)
                    };
                    self.execution_contexts.insert(
                        (sid, ctx_id),
                        ExecutionContextMeta { origin, is_default, ctx_type, frame_id },
                    );
                }
            }
            "Runtime.executionContextDestroyed" => {
                if let Some(sid) = session_id
                    && let Some(ctx_id) = params.get("executionContextId").and_then(Value::as_i64)
                {
                    self.execution_contexts.remove(&(sid.clone(), ctx_id));
                    if self.sessions.get(&sid).is_some_and(|s| s.kind == SessionKind::ServiceWorker)
                        && !self.execution_contexts.keys().any(|(s, _)| s == &sid)
                    {
                        self.remove_session_subscribers(&sid);
                    }
                }
            }
            "Runtime.executionContextsCleared" => {
                if let Some(sid) = session_id {
                    self.execution_contexts.retain(|(s, _), _| s != &sid);
                    if self.sessions.get(&sid).is_some_and(|s| s.kind == SessionKind::ServiceWorker) {
                        self.remove_session_subscribers(&sid);
                    }
                }
            }
            "Inspector.targetCrashed" => {
                if let Some(sid) = session_id
                    && let Some(sess) = self.sessions.get_mut(&sid)
                {
                    sess.shimmed = false;
                    sess.pending_shim_eval_id = None;
                    sess.setup_failed = false;
                    sess.setup_pending = 0;
                    sess.resume_when_ready = None;
                    sess.fetch_confirmed = false;
                    self.execution_contexts.retain(|(s, _), _| s != &sid);
                    self.pending_setup_cmds.retain(|_, s| s != &sid);
                    self.pending_fetch_resume.retain(|_, (s, _)| s != &sid);
                    self.pending_loaded_resume.retain(|(s, _)| s != &sid);
                    self.remove_session_subscribers(&sid);
                }
            }
            "Inspector.targetReloadedAfterCrash" => {
                if let Some(sid) = session_id
                    && let Some((kind, is_iframe)) = self.sessions.get_mut(&sid).map(|sess| {
                        sess.attach_started = Instant::now();
                        sess.pause_started = None;
                        sess.pending_shim_eval_id = None;
                        sess.was_paused = true;
                        sess.setup_failed = false;
                        sess.setup_pending = 0;
                        sess.resume_when_ready = None;
                        sess.fetch_confirmed = false;
                        sess.shimmed = false;
                        (sess.kind, sess.is_iframe)
                    })
                {
                    self.execution_contexts.retain(|(s, _), _| s != &sid);
                    self.pending_setup_cmds.retain(|_, s| s != &sid);
                    self.pending_fetch_resume.retain(|_, (s, _)| s != &sid);
                    self.pending_loaded_resume.retain(|(s, _)| s != &sid);
                    self.remove_session_subscribers(&sid);
                    let fetch_id = self.alloc_id();
                    self.pending_fetch_resume.insert(fetch_id, (sid.clone(), true));
                    let batch = match kind {
                        SessionKind::ServiceWorker => vec![
                            json!({ "id": self.alloc_setup_id(&sid), "sessionId": sid, "method": "Runtime.enable", "params": {} }),
                            auto_attach_cmd(self.alloc_setup_id(&sid), &sid),
                            json!({ "id": self.alloc_setup_id(&sid), "sessionId": sid, "method": "Runtime.addBinding", "params": { "name": "__ocExtCall" } }),
                            json!({ "id": self.alloc_setup_id(&sid), "sessionId": sid, "method": "Debugger.enable", "params": {} }),
                            json!({ "id": self.alloc_setup_id(&sid), "sessionId": sid, "method": "EventBreakpoints.setInstrumentationBreakpoint", "params": { "eventName": "scriptFirstStatement" } }),
                            fetch_enable_cmd(fetch_id, &sid),
                        ],
                        SessionKind::FileGuardOnly => {
                            let mut cmds = vec![auto_attach_cmd(self.alloc_setup_id(&sid), &sid)];
                            if is_iframe {
                                // Re-apply iframe API lockdown on crash reload (READER-7 #592 R2).
                                cmds.push(json!({ "id": self.alloc_setup_id(&sid), "sessionId": sid, "method": "Page.enable", "params": {} }));
                                cmds.push(json!({
                                    "id": self.alloc_setup_id(&sid),
                                    "sessionId": sid,
                                    "method": "Page.addScriptToEvaluateOnNewDocument",
                                    "params": { "source": IFRAME_LOCKDOWN_JS }
                                }));
                            }
                            cmds.push(fetch_enable_cmd(fetch_id, &sid));
                            cmds
                        }
                        SessionKind::ExtensionPage | SessionKind::PopupPage | SessionKind::SeedProbe => {
                            let mut cmds = vec![
                                json!({ "id": self.alloc_setup_id(&sid), "sessionId": sid, "method": "Runtime.enable", "params": {} }),
                                json!({ "id": self.alloc_setup_id(&sid), "sessionId": sid, "method": "Page.enable", "params": {} }),
                                auto_attach_cmd(self.alloc_setup_id(&sid), &sid),
                            ];
                            if kind != SessionKind::SeedProbe {
                                let ext_id = self.sessions.get(&sid).map(|s| s.ext_id.clone()).unwrap_or_default();
                                let perms = self.extensions.get(&ext_id).map(|e| e.manifest.permissions.as_slice()).unwrap_or(&[]);
                                let code = shim_source(kind == SessionKind::PopupPage, perms);
                                cmds.push(json!({ "id": self.alloc_setup_id(&sid), "sessionId": sid, "method": "Runtime.addBinding", "params": { "name": "__ocExtCall" } }));
                                cmds.push(json!({ "id": self.alloc_setup_id(&sid), "sessionId": sid, "method": "Page.addScriptToEvaluateOnNewDocument", "params": { "source": code } }));
                            }
                            cmds.push(fetch_enable_cmd(fetch_id, &sid));
                            cmds
                        }
                    };
                    // `Runtime.runIfWaitingForDebugger` is deferred until BOTH `fetch_id` AND every
                    // setup command in `batch` reply cleanly (READER-7 #592 R1).
                    self.pipe.send_batch(&batch);
                }
            }
            "Debugger.paused" => {
                if let Some(sid) = session_id
                    && let Some((kind, ext_id)) = self.sessions.get(&sid).map(|s| (s.kind, s.ext_id.clone()))
                {
                    let perms = self.extensions.get(&ext_id).map(|e| e.manifest.permissions.as_slice()).unwrap_or(&[]);
                    let code = shim_source(kind == SessionKind::PopupPage, perms);
                    let pause_now = Instant::now();
                    let eval_id = self.send_cdp(
                        "Runtime.evaluate",
                        json!({ "expression": code, "returnByValue": true }),
                        Some(&sid),
                    );
                    if let Some(s) = self.sessions.get_mut(&sid) {
                        s.pause_started = Some(pause_now);
                        s.pending_shim_eval_id = Some(eval_id);
                    }
                }
            }
            "Runtime.bindingCalled" => {
                if let Some(sid) = session_id
                    && params.get("name").and_then(Value::as_str) == Some("__ocExtCall")
                    && let Some(payload_str) = params.get("payload").and_then(Value::as_str)
                    && payload_str.len() <= MAX_BINDING_PAYLOAD_BYTES
                    && let Ok(payload) = serde_json::from_str::<Value>(payload_str)
                {
                    // Defense-in-depth (`review-rd274.md` §3):
                    // 1. Require tracked `executionContextId` origin == `chrome-extension://<ext_id>`.
                    // 2. For pages/popups, strictly require `isDefault == true && type == "default"`
                    //    (rejecting content-script isolated worlds) AND main frame match.
                    // 3. Drop invalid sub-frame / isolated-world calls without detaching the parent
                    //    extension page (prevents a web iframe inside an extension page from DoS-ing it).
                    let ctx_id = params.get("executionContextId").and_then(Value::as_i64);
                    let check = (|| -> Option<(bool, bool)> {
                        let sess = self.sessions.get(&sid)?;
                        let meta = ctx_id.and_then(|cid| self.execution_contexts.get(&(sid.clone(), cid)))?;
                        let origin_ok = extension_id_from_url(&meta.origin) == Some(sess.ext_id.as_str());
                        let world_ok = match sess.kind {
                            SessionKind::ServiceWorker => true,
                            SessionKind::FileGuardOnly | SessionKind::SeedProbe => false,
                            SessionKind::ExtensionPage | SessionKind::PopupPage => {
                                let frame_ok = sess.main_frame_id.is_none() || meta.frame_id == sess.main_frame_id;
                                meta.is_default && meta.ctx_type == "default" && frame_ok
                            }
                        };
                        let is_main_world_violation = sess.kind == SessionKind::ServiceWorker
                            || (meta.is_default
                                && (sess.main_frame_id.is_none() || meta.frame_id == sess.main_frame_id));
                        Some((origin_ok && world_ok, is_main_world_violation))
                    })();
                    match check {
                        Some((true, _)) => effects.extend(self.handle_binding_call(&sid, payload)),
                        Some((false, true)) => self.detach_session(&sid),
                        _ => {}
                    }
                }
            }
            "Page.loadEventFired" => {
                if let Some(sid) = session_id
                    && self.sessions.get(&sid).is_some_and(|s| s.kind == SessionKind::SeedProbe)
                    && self.seed_eval_id.is_none()
                {
                    let eval_id = self.send_cdp(
                        "Runtime.evaluate",
                        json!({
                            "expression": "new Promise(r => chrome.tabs.getCurrent(t => r(t ? t.id : -1)))",
                            "awaitPromise": true,
                            "returnByValue": true
                        }),
                        Some(&sid),
                    );
                    self.seed_eval_id = Some(eval_id);
                }
            }
            _ => {}
        }
        effects
    }

    pub(super) fn tick_timeouts(&mut self) -> Vec<ExtEffect> {
        let mut effects = Vec::new();
        let expired: Vec<(String, String)> = self
            .probe_in_flight
            .iter()
            .filter(|(_, p)| p.sent_at.elapsed() >= PROBE_TIMEOUT)
            .map(|(ext_id, p)| (ext_id.clone(), p.sid.clone()))
            .collect();
        for (ext_id, stale_sid) in expired {
            self.failed_probe_sessions.insert(stale_sid);
            self.probe_in_flight.remove(&ext_id);
            self.trigger_tab_id_probe_for(&ext_id);
            if !self.probe_in_flight.contains_key(&ext_id) {
                effects.extend(self.flush_pending_rpcs_for(&ext_id));
            }
        }
        effects
    }

    pub(super) fn on_frame_dropped(&mut self) -> Vec<ExtEffect> {
        let mut effects = Vec::new();
        let dropped: Vec<(String, ProbeInFlight)> = self.probe_in_flight.drain().collect();
        for (ext_id, probe) in dropped {
            self.failed_probe_sessions.insert(probe.sid);
            self.trigger_tab_id_probe_for(&ext_id);
            if !self.probe_in_flight.contains_key(&ext_id) {
                effects.extend(self.flush_pending_rpcs_for(&ext_id));
            }
        }
        effects
    }

    pub(super) fn on_pipe_closed(&mut self) -> Vec<ExtEffect> {
        let mut effects = Vec::new();
        self.probe_in_flight.clear();
        self.pending_rpcs.clear();
        for (ext_id, ids) in std::mem::take(&mut self.notifications) {
            effects.extend(ids.into_iter().map(|notif_id| ExtEffect::NotificationClear { ext_id: ext_id.clone(), notif_id }));
        }
        if !self.filters_ready_emitted {
            self.filters_ready_emitted = true;
            effects.push(ExtEffect::FiltersReady);
        }
        for (_, ext_id) in std::mem::take(&mut self.pending_uninstall_cmd) {
            effects.push(ExtEffect::ExtensionUninstalled {
                ext_id,
                ok: false,
                error: Some("Extension CDP pipe closed.".into()),
            });
        }
        for (_, ext_id) in std::mem::take(&mut self.pending_uninstall_upgrade) {
            effects.push(ExtEffect::ExtensionUninstalled {
                ext_id,
                ok: false,
                error: Some("Extension CDP pipe closed.".into()),
            });
        }
        for (_, ext_id) in std::mem::take(&mut self.pending_reload_cmd) {
            self.extensions.remove(&ext_id);
            effects.push(ExtEffect::ExtensionReloaded {
                ext_id,
                ok: false,
                error: Some("Extension CDP pipe closed.".into()),
            });
        }
        effects
    }

    fn detach_session(&mut self, sid: &str) {
        self.cleanup_detached_session(sid);
        let _ = self.send_cdp("Target.detachFromTarget", json!({ "sessionId": sid }), None);
    }

    fn cleanup_detached_session(&mut self, sid: &str) {
        if let Some(removed) = self.sessions.remove(sid)
            && removed.kind == SessionKind::ServiceWorker
            && !removed.target_id.is_empty()
            && let Some(dups) = self.deferred_dup_sw_detaches.remove(&removed.target_id)
        {
            // Primary SW detached before `Fetch.enable` confirmed: detach any queued duplicate
            // sessions WITHOUT sending `Runtime.runIfWaitingForDebugger` (fail closed).
            for (dup_sid, _) in dups {
                let _ = self.send_cdp("Target.detachFromTarget", json!({ "sessionId": dup_sid }), None);
            }
        }
        self.deferred_dup_sw_detaches.retain(|_, list| {
            list.retain(|(dup_sid, _)| dup_sid != sid);
            !list.is_empty()
        });
        self.execution_contexts.retain(|(s, _), _| s != sid);
        self.pending_setup_cmds.retain(|_, s| s != sid);
        self.pending_fetch_resume.retain(|_, (s, _)| s != sid);
        self.pending_loaded_resume.retain(|(s, _)| s != sid);
        self.failed_probe_sessions.remove(sid);
        let detached_probe_ext = self
            .probe_in_flight
            .iter()
            .find(|(_, p)| p.sid == sid)
            .map(|(ext_id, _)| ext_id.clone());
        if let Some(ext_id) = detached_probe_ext {
            self.probe_in_flight.remove(&ext_id);
            self.trigger_tab_id_probe_for(&ext_id);
        }
        self.remove_session_subscribers(sid);
    }

    fn remove_session_subscribers(&mut self, sid: &str) {
        self.subscribers.retain(|_, by_ext| {
            by_ext.retain(|_, set| {
                set.remove(sid);
                !set.is_empty()
            });
            !by_ext.is_empty()
        });
    }

    fn alloc_setup_id(&mut self, sid: &str) -> i64 {
        let id = self.alloc_id();
        self.pending_setup_cmds.insert(id, sid.to_owned());
        if let Some(sess) = self.sessions.get_mut(sid) {
            sess.setup_pending = sess.setup_pending.saturating_add(1);
        }
        id
    }

    fn try_complete_session_setup(&mut self, sid: &str) {
        let ready = self.sessions.get_mut(sid).and_then(|sess| {
            if sess.setup_failed || sess.setup_pending > 0 {
                return None;
            }
            let should_resume = sess.resume_when_ready.take()?;
            sess.fetch_confirmed = true;
            Some((sess.ext_id.clone(), should_resume))
        });
        if let Some((ext_id, should_resume)) = ready {
            if self.extensions.get(&ext_id).is_some_and(|e| e.loaded) {
                self.resume_confirmed_session(sid, should_resume);
            } else if !self.pending_loaded_resume.iter().any(|(s, _)| s == sid) {
                self.pending_loaded_resume.push((sid.to_owned(), should_resume));
            }
        }
    }

    fn handle_cdp_reply(&mut self, id: i64, msg: &Value) -> Vec<ExtEffect> {
        let mut effects = Vec::new();
        if let Some(sid) = self.pending_setup_cmds.remove(&id) {
            if let Some(sess) = self.sessions.get_mut(&sid) {
                sess.setup_pending = sess.setup_pending.saturating_sub(1);
                if msg.get("error").is_some() {
                    sess.setup_failed = true;
                    sess.resume_when_ready = None;
                }
            }
            self.try_complete_session_setup(&sid);
            return effects;
        }
        if let Some((sid, should_resume)) = self.pending_fetch_resume.remove(&id) {
            // Fail closed if `Fetch.enable` OR any earlier setup command returned an error:
            // NEVER resume an unguarded target (`review-rd274.md` Round-3 §1, #592 R1), and
            // hold resume until `ext.loaded == true` (READER-7 F2).
            if msg.get("error").is_some() {
                if let Some(sess) = self.sessions.get_mut(&sid) {
                    sess.setup_failed = true;
                    sess.resume_when_ready = None;
                }
            } else if let Some(sess) = self.sessions.get_mut(&sid)
                && !sess.setup_failed
            {
                sess.resume_when_ready = Some(should_resume);
            }
            self.try_complete_session_setup(&sid);
            return effects;
        }
        if let Some(bad_id) = self.pending_stub_cleanup.remove(&id) {
            if msg.get("error").is_none() {
                let _ = self.send_cdp("Extensions.uninstall", json!({ "id": bad_id }), None);
            }
            return effects;
        }
        if let Some(expected_id) = self.pending_startup_loads.remove(&id) {
            let ok = msg.get("error").is_none();
            let loaded_id = msg.pointer("/result/id").and_then(Value::as_str).unwrap_or("");
            // Post-load fail-closed verification: confirm Chromium actually cleared
            // `Extension::ALLOW_FILE_ACCESS` (`creation_flags & 4 == 0` and
            // `newAllowFileAccess == false`) AND granted zero permissions outside
            // `ALLOWED_CHROMIUM_PERMS` under `<loaded_id>` (`review-rd274.md` Round-3 §1a).
            let file_access_revoked =
                ok && loaded_id == expected_id && crate::context::verify_extension_file_access_disabled(loaded_id);
            if file_access_revoked {
                if let Some(ext) = self.extensions.get_mut(&expected_id) {
                    ext.is_unpacked = true;
                    ext.loaded = true;
                }
                self.resume_sessions_loaded_for(&expected_id);
                if let Some(snap) = self.action_snapshot(&expected_id) {
                    effects.push(ExtEffect::ActionUpdated(snap));
                }
                if let Some(pos) = self.queued_clicks.iter().position(|cid| cid == &expected_id) {
                    self.queued_clicks.remove(pos);
                    effects.extend(self.click_extension(&expected_id));
                }
            } else {
                self.drop_loaded_resumes_for(&expected_id);
                self.sw_wake.remove_extension(&expected_id, true);
                if let Some(removed) = self.extensions.remove(&expected_id)
                    && let Some(staged) = removed.staged_dir
                {
                    super::integrity::cleanup_staged_dir(&staged);
                }
                self.verified_startup.remove(&expected_id);
                self.actions.remove(&expected_id);
                self.queued_clicks.retain(|cid| cid != &expected_id);
                let stale_sids: Vec<String> = self
                    .sessions
                    .iter()
                    .filter(|(_, s)| s.ext_id == expected_id || s.ext_id == loaded_id)
                    .map(|(sid, _)| sid.clone())
                    .collect();
                for sid in stale_sids {
                    self.detach_session(&sid);
                }
                if is_valid_ext_id(loaded_id) {
                    let _ = self.send_cdp("Extensions.uninstall", json!({ "id": loaded_id }), None);
                }
                if loaded_id != expected_id {
                    self.uninstall_inert_stub(&expected_id);
                }
                effects.extend(self.forget_notifications(&expected_id));
                effects.push(ExtEffect::IntegrityFailed { ext_id: expected_id.clone() });
            }
            self.pending_filter_ids.remove(&expected_id);
            if self.pending_filter_ids.is_empty() && !self.filters_ready_emitted {
                self.filters_ready_emitted = true;
                effects.push(ExtEffect::FiltersReady);
            }
            if self.pending_startup_loads.is_empty() {
                effects.push(ExtEffect::ExtensionsSynced(self.list_actions()));
            }
            return effects;
        }
        if let Some(ext_id) = self.pending_uninstall_upgrade.remove(&id) {
            if msg.get("error").is_none() {
                if let Some(ext) = self.extensions.get_mut(&ext_id) {
                    ext.is_unpacked = true;
                }
                let cid = self.send_cdp("Extensions.uninstall", json!({ "id": ext_id }), None);
                self.pending_uninstall_cmd.insert(cid, ext_id);
            } else {
                effects.push(ExtEffect::ExtensionUninstalled {
                    ext_id,
                    ok: false,
                    error: msg.get("error").map(Value::to_string),
                });
            }
            return effects;
        }
        if let Some(ext_id) = self.pending_uninstall_cmd.remove(&id) {
            let ok = msg.get("error").is_none();
            if ok {
                self.drop_loaded_resumes_for(&ext_id);
                self.sw_wake.remove_extension(&ext_id, true);
                if let Some(removed) = self.extensions.remove(&ext_id)
                    && let Some(staged) = removed.staged_dir
                {
                    super::integrity::cleanup_staged_dir(&staged);
                }
                self.actions.remove(&ext_id);
                self.context_menus.lock().unwrap().remove_extension(&ext_id);
                effects.extend(self.forget_notifications(&ext_id));
                self.ext_tab_ids.remove(&ext_id);
                self.ext_last_known_session_id.remove(&ext_id);
                self.announced_active.remove(&ext_id);
                self.probe_in_flight.remove(&ext_id);
                self.subscribers.retain(|_, by_ext| {
                    by_ext.remove(&ext_id);
                    !by_ext.is_empty()
                });
            }
            effects.push(ExtEffect::ExtensionUninstalled {
                ext_id,
                ok,
                error: msg.get("error").map(Value::to_string),
            });
            return effects;
        }
        if let Some(expected_id) = self.pending_reload_cmd.remove(&id) {
            let ok = msg.get("error").is_none();
            let loaded_id = msg.pointer("/result/id").and_then(Value::as_str).unwrap_or("").to_owned();
            let file_access_revoked = ok
                && is_valid_ext_id(&loaded_id)
                && loaded_id == expected_id
                && crate::context::verify_extension_file_access_disabled(&loaded_id);
            if file_access_revoked {
                if let Some(ext) = self.extensions.get_mut(&expected_id) {
                    ext.is_unpacked = true;
                    ext.loaded = true;
                }
                self.resume_sessions_loaded_for(&expected_id);
                effects.push(ExtEffect::ExtensionReloaded { ext_id: loaded_id, ok: true, error: None });
            } else {
                self.drop_loaded_resumes_for(&expected_id);
                self.sw_wake.remove_extension(&expected_id, true);
                if let Some(removed) = self.extensions.remove(&expected_id)
                    && let Some(staged) = removed.staged_dir
                {
                    super::integrity::cleanup_staged_dir(&staged);
                }
                self.verified_startup.remove(&expected_id);
                let stale_sids: Vec<String> = self
                    .sessions
                    .iter()
                    .filter(|(_, s)| s.ext_id == expected_id || s.ext_id == loaded_id)
                    .map(|(sid, _)| sid.clone())
                    .collect();
                for sid in stale_sids {
                    self.detach_session(&sid);
                }
                if is_valid_ext_id(&loaded_id) {
                    let _ = self.send_cdp("Extensions.uninstall", json!({ "id": loaded_id }), None);
                }
                effects.extend(self.forget_notifications(&expected_id));
                effects.push(ExtEffect::ExtensionReloaded {
                    ext_id: expected_id,
                    ok: false,
                    error: msg
                        .get("error")
                        .map(Value::to_string)
                        .or_else(|| Some("Loaded ID mismatch or file:// access not revoked".into())),
                });
            }
            return effects;
        }
        if self.seed_eval_id == Some(id) {
            self.seed_eval_id = None;
            if let Some(seed_val) = msg.pointer("/result/result/value").and_then(Value::as_i64)
                && seed_val > 0
                && seed_val < (i32::MAX - 1024) as i64
            {
                self.seed_session_id = Some(seed_val as i32);
                self.trigger_tab_id_probes_all();
            }
            effects.push(ExtEffect::CloseSeedProbe);
            return effects;
        }

        let matching_sid = self
            .sessions
            .iter()
            .find(|(_, s)| s.pending_shim_eval_id == Some(id))
            .map(|(sid, _)| sid.clone());
        if let Some(sid) = matching_sid {
            let val = msg.pointer("/result/result/value").and_then(Value::as_str).unwrap_or("");
            let was_paused = self.sessions.get(&sid).is_some_and(|s| s.was_paused);
            if val == "WAIT" {
                if let Some(s) = self.sessions.get_mut(&sid) {
                    s.pending_shim_eval_id = None;
                }
                if was_paused {
                    self.send_cdp("Debugger.resume", json!({}), Some(&sid));
                }
            } else if val == "DENIED" {
                self.cleanup_detached_session(&sid);
                let mut batch = Vec::new();
                if was_paused {
                    batch.push(json!({ "id": self.alloc_id(), "sessionId": sid, "method": "Debugger.disable", "params": {} }));
                    batch.push(json!({ "id": self.alloc_id(), "sessionId": sid, "method": "Debugger.resume", "params": {} }));
                }
                batch.push(json!({ "id": self.alloc_id(), "method": "Target.detachFromTarget", "params": { "sessionId": sid } }));
                self.pipe.send_batch(&batch);
            } else {
                let mut is_sw = false;
                let mut ext_id = String::new();
                if let Some(sess) = self.sessions.get_mut(&sid) {
                    sess.pending_shim_eval_id = None;
                    sess.shimmed = true;
                    let total_us = sess.attach_started.elapsed().as_micros() as u64;
                    let eval_us = sess.pause_started.map_or(0, |t| t.elapsed().as_micros() as u64);
                    is_sw = sess.kind == SessionKind::ServiceWorker;
                    ext_id = sess.ext_id.clone();
                    if is_sw {
                        self.metrics.sw_boots += 1;
                        push_bounded(&mut self.metrics.sw_attach_to_resume_us, total_us);
                        push_bounded(&mut self.metrics.sw_shim_eval_us, eval_us);
                    } else {
                        self.metrics.page_boots += 1;
                        push_bounded(&mut self.metrics.page_attach_to_resume_us, total_us);
                        push_bounded(&mut self.metrics.page_shim_eval_us, eval_us);
                    }
                }
                if was_paused {
                    let batch = vec![
                        json!({
                            "id": self.alloc_id(),
                            "sessionId": sid,
                            "method": "EventBreakpoints.removeInstrumentationBreakpoint",
                            "params": { "eventName": "scriptFirstStatement" }
                        }),
                        json!({ "id": self.alloc_id(), "sessionId": sid, "method": "Debugger.disable", "params": {} }),
                        json!({ "id": self.alloc_id(), "sessionId": sid, "method": "Debugger.resume", "params": {} }),
                    ];
                    self.pipe.send_batch(&batch);
                }
                if let Some(cid) = self.active_chromium_tab_id_for(&ext_id) {
                    self.push_to_session(&sid, &json!({ "kind": "active_tab", "tabId": cid }));
                }
                if is_sw {
                    effects.extend(self.maybe_start_seed_probe());
                    if let Some(ext) = self.extensions.get_mut(&ext_id) {
                        ext.boot_count += 1;
                        if ext.boot_count > 1 {
                            self.metrics.sw_restarts += 1;
                        }
                    }
                }
                self.trigger_tab_id_probe_for(&ext_id);
            }
        }
        effects
    }

    fn consume_expected_for_ext(&mut self, ext_id: &str, url: &str) -> Option<SessionKind> {
        let is_manifest = url.ends_with("/manifest.json") && self.seed_session_id.is_none();
        let pos = self.expected_targets.iter().position(|t| match t {
            ExpectedTarget::SeedProbe { ext_id: id } => is_manifest && id == ext_id,
            ExpectedTarget::Popup { ext_id: id } => !is_manifest && id == ext_id,
            ExpectedTarget::ExtensionPage { ext_id: id } => !is_manifest && id == ext_id,
            _ => false,
        });
        pos.and_then(|i| match self.expected_targets.remove(i)? {
            ExpectedTarget::SeedProbe { .. } => Some(SessionKind::SeedProbe),
            ExpectedTarget::Popup { .. } => Some(SessionKind::PopupPage),
            ExpectedTarget::ExtensionPage { .. } => Some(SessionKind::ExtensionPage),
            _ => None,
        })
    }

    fn handle_attached(&mut self, params: &Value) -> Vec<ExtEffect> {
        let mut effects = Vec::new();
        let Some(sid) = params.get("sessionId").and_then(Value::as_str) else {
            return effects;
        };
        let waiting = params.get("waitingForDebugger").and_then(Value::as_bool).unwrap_or(false);
        let tinfo = params.get("targetInfo").unwrap_or(&Value::Null);
        let ttype = tinfo.get("type").and_then(Value::as_str).unwrap_or("");
        let target_id = tinfo.get("targetId").and_then(Value::as_str).unwrap_or("").to_owned();
        let url = tinfo.get("url").and_then(Value::as_str).unwrap_or("");

        // 1. Extension service worker: match strictly by verified extension ID.
        if ttype == "service_worker"
            && let Some(ext_id) = extension_id_from_url(url)
            && self.extensions.contains_key(ext_id)
        {
            // Never attach a second concurrent CDP session to the same service-worker target
            // (e.g. from a child page's `Target.setAutoAttach` when the browser target already
            // attached it), which causes competing `Debugger`/`Runtime.evaluate` races.
            // Until BOTH the primary session's `Fetch.enable` confirms (`fetch_confirmed == true`)
            // AND post-load `verify_extension_file_access_disabled` passes (`ext.loaded == true`),
            // hold this duplicate quiet (never resuming it); if either fails or never confirms,
            // the worker stays paused forever (fail closed).
            if !target_id.is_empty()
                && let Some(primary_fetch_confirmed) = self
                    .sessions
                    .values()
                    .find(|s| s.kind == SessionKind::ServiceWorker && s.target_id == target_id)
                    .map(|s| s.fetch_confirmed)
            {
                let ext_loaded = self.extensions.get(ext_id).is_some_and(|e| e.loaded);
                if !primary_fetch_confirmed || !ext_loaded {
                    let list = self.deferred_dup_sw_detaches.entry(target_id).or_default();
                    if list.len() < 64 {
                        list.push((sid.to_owned(), waiting));
                    }
                } else {
                    let mut batch = Vec::new();
                    if waiting {
                        batch.push(json!({ "id": self.alloc_id(), "sessionId": sid, "method": "Runtime.runIfWaitingForDebugger", "params": {} }));
                    }
                    batch.push(json!({ "id": self.alloc_id(), "method": "Target.detachFromTarget", "params": { "sessionId": sid } }));
                    self.pipe.send_batch(&batch);
                }
                return effects;
            }
            let ext_id = ext_id.to_owned();
            self.sessions.insert(
                sid.to_owned(),
                AttachedSession {
                    ext_id: ext_id.clone(),
                    target_id,
                    kind: SessionKind::ServiceWorker,
                    url: url.to_owned(),
                    chromium_tab_id: None,
                    main_frame_id: None,
                    last_popup_size: None,
                    attach_started: Instant::now(),
                    pause_started: None,
                    pending_shim_eval_id: None,
                    was_paused: waiting,
                    is_iframe: false,
                    setup_failed: false,
                    setup_pending: 0,
                    resume_when_ready: None,
                    fetch_confirmed: false,
                    shimmed: false,
                },
            );
            if waiting {
                let fetch_id = self.alloc_id();
                self.pending_fetch_resume.insert(fetch_id, (sid.to_owned(), true));
                // Track every pre-resume setup command + `Fetch.enable` so resume fires strictly
                // after all confirm cleanly (fail-closed on any setup error, READER-7 #592 R1).
                let batch = vec![
                    json!({ "id": self.alloc_setup_id(sid), "sessionId": sid, "method": "Runtime.enable", "params": {} }),
                    auto_attach_cmd(self.alloc_setup_id(sid), sid),
                    json!({ "id": self.alloc_setup_id(sid), "sessionId": sid, "method": "Runtime.addBinding", "params": { "name": "__ocExtCall" } }),
                    json!({ "id": self.alloc_setup_id(sid), "sessionId": sid, "method": "Debugger.enable", "params": {} }),
                    json!({ "id": self.alloc_setup_id(sid), "sessionId": sid, "method": "EventBreakpoints.setInstrumentationBreakpoint", "params": { "eventName": "scriptFirstStatement" } }),
                    fetch_enable_cmd(fetch_id, sid),
                ];
                // Resume happens strictly in `handle_cdp_reply` after all setup commands + `fetch_id` confirm.
                self.pipe.send_batch(&batch);
            } else {
                let fetch_id = self.alloc_id();
                self.pending_fetch_resume.insert(fetch_id, (sid.to_owned(), false));
                let eval_id = self.alloc_id();
                if let Some(s) = self.sessions.get_mut(sid) {
                    s.pause_started = Some(Instant::now());
                    s.pending_shim_eval_id = Some(eval_id);
                }
                let perms = self.extensions.get(&ext_id).map(|e| e.manifest.permissions.as_slice()).unwrap_or(&[]);
                let code = shim_source(false, perms);
                let batch = vec![
                    json!({ "id": self.alloc_setup_id(sid), "sessionId": sid, "method": "Runtime.enable", "params": {} }),
                    auto_attach_cmd(self.alloc_setup_id(sid), sid),
                    fetch_enable_cmd(fetch_id, sid),
                    json!({ "id": self.alloc_setup_id(sid), "sessionId": sid, "method": "Runtime.addBinding", "params": { "name": "__ocExtCall" } }),
                    json!({ "id": eval_id, "sessionId": sid, "method": "Runtime.evaluate", "params": { "expression": code, "returnByValue": true } }),
                ];
                self.pipe.send_batch(&batch);
            }
            effects.extend(self.maybe_start_seed_probe());
            return effects;
        }

        // Extension dedicated/shared workers (including `blob:chrome-extension://<id>/...` workers)
        // and extension iframes: attach ONLY `Target.setAutoAttach` + `Fetch.enable`
        // (never install `__ocExtCall` or `shim.js`), and await `Fetch.enable`'s reply before
        // resuming (`review-rd274.md` Round-2 §1a-c & Round-3 §1).
        if matches!(ttype, "worker" | "shared_worker" | "iframe")
            && let Some(ext_id) = super::manifest::extension_id_from_target_url(url)
            && self.extensions.contains_key(ext_id)
        {
            self.sessions.insert(
                sid.to_owned(),
                AttachedSession {
                    ext_id: ext_id.to_owned(),
                    target_id,
                    kind: SessionKind::FileGuardOnly,
                    url: url.to_owned(),
                    chromium_tab_id: None,
                    main_frame_id: None,
                    last_popup_size: None,
                    attach_started: Instant::now(),
                    pause_started: None,
                    pending_shim_eval_id: None,
                    was_paused: false,
                    is_iframe: ttype == "iframe",
                    setup_failed: false,
                    setup_pending: 0,
                    resume_when_ready: None,
                    fetch_confirmed: false,
                    shimmed: false,
                },
            );
            let fetch_id = self.alloc_id();
            self.pending_fetch_resume.insert(fetch_id, (sid.to_owned(), waiting));
            let mut batch = vec![auto_attach_cmd(self.alloc_setup_id(sid), sid)];
            if ttype == "iframe" {
                // Neutralize native `chrome.tabs`/`windows`/etc. inside extension iframes while
                // keeping `chrome.runtime` native (never installs `__ocExtCall`, READER-12 C2).
                batch.push(json!({ "id": self.alloc_setup_id(sid), "sessionId": sid, "method": "Page.enable", "params": {} }));
                batch.push(json!({
                    "id": self.alloc_setup_id(sid),
                    "sessionId": sid,
                    "method": "Page.addScriptToEvaluateOnNewDocument",
                    "params": { "source": IFRAME_LOCKDOWN_JS }
                }));
            }
            batch.push(fetch_enable_cmd(fetch_id, sid));
            self.pipe.send_batch(&batch);
            return effects;
        }

        // Never attach shims to iframes, dedicated/shared workers, or non-page targets;
        // include MV2 `background_page` so it is never detached unshimmed (READER-12 #441).
        let is_page_like = matches!(ttype, "page" | "other" | "background_page");
        let ext_target = if is_page_like
            && let Some(ext_id) = super::manifest::extension_id_from_target_url(url)
            && self.extensions.contains_key(ext_id)
        {
            // Identity match: also consume any matching queued expected target so it is never stranded.
            let kind = self.consume_expected_for_ext(ext_id, url).unwrap_or_else(|| {
                if url.ends_with("/manifest.json") && self.seed_session_id.is_none() {
                    SessionKind::SeedProbe
                } else {
                    SessionKind::ExtensionPage
                }
            });
            Some((ext_id.to_owned(), kind))
        } else if is_page_like && (url.is_empty() || url == "about:blank") {
            match self.expected_targets.pop_front() {
                Some(ExpectedTarget::Popup { ext_id }) if self.extensions.contains_key(&ext_id) => {
                    Some((ext_id, SessionKind::PopupPage))
                }
                Some(ExpectedTarget::ExtensionPage { ext_id }) if self.extensions.contains_key(&ext_id) => {
                    Some((ext_id, SessionKind::ExtensionPage))
                }
                Some(ExpectedTarget::SeedProbe { ext_id }) if self.extensions.contains_key(&ext_id) => {
                    Some((ext_id, SessionKind::SeedProbe))
                }
                _ => None,
            }
        } else {
            None
        };

        if let Some((ext_id, kind)) = ext_target {
            self.sessions.insert(
                sid.to_owned(),
                AttachedSession {
                    ext_id: ext_id.clone(),
                    target_id,
                    kind,
                    url: url.to_owned(),
                    chromium_tab_id: None,
                    main_frame_id: None,
                    last_popup_size: None,
                    attach_started: Instant::now(),
                    pause_started: None,
                    pending_shim_eval_id: None,
                    was_paused: false,
                    is_iframe: false,
                    setup_failed: false,
                    setup_pending: 0,
                    resume_when_ready: None,
                    fetch_confirmed: false,
                    shimmed: false,
                },
            );
            let fetch_id = self.alloc_id();
            self.pending_fetch_resume.insert(fetch_id, (sid.to_owned(), waiting));
            let mut batch = vec![
                json!({ "id": self.alloc_setup_id(sid), "sessionId": sid, "method": "Runtime.enable", "params": {} }),
                json!({ "id": self.alloc_setup_id(sid), "sessionId": sid, "method": "Page.enable", "params": {} }),
                auto_attach_cmd(self.alloc_setup_id(sid), sid),
            ];
            if kind != SessionKind::SeedProbe {
                let perms = self.extensions.get(&ext_id).map(|e| e.manifest.permissions.as_slice()).unwrap_or(&[]);
                let code = shim_source(kind == SessionKind::PopupPage, perms);
                batch.push(json!({ "id": self.alloc_setup_id(sid), "sessionId": sid, "method": "Runtime.addBinding", "params": { "name": "__ocExtCall" } }));
                batch.push(json!({ "id": self.alloc_setup_id(sid), "sessionId": sid, "method": "Page.addScriptToEvaluateOnNewDocument", "params": { "source": code } }));
                if let Some(s) = self.sessions.get_mut(sid) {
                    self.metrics.page_boots += 1;
                    push_bounded(
                        &mut self.metrics.page_attach_to_resume_us,
                        s.attach_started.elapsed().as_micros() as u64,
                    );
                }
            }
            batch.push(fetch_enable_cmd(fetch_id, sid));
            // Resume happens strictly in `handle_cdp_reply` after all setup commands + `fetch_id` confirm.
            self.pipe.send_batch(&batch);
            return effects;
        }

        // Regular web tabs, isolated tabs, iframes, site workers: resume & detach immediately.
        // Consume one matching queued `WebTab`/`IsolatedTab` when a page-like non-empty web tab
        // attaches so stale web expectations never desync a later pre-commit extension page.
        if is_page_like
            && !url.is_empty()
            && url != "about:blank"
            && let Some(pos) = self
                .expected_targets
                .iter()
                .position(|t| matches!(t, ExpectedTarget::WebTab | ExpectedTarget::IsolatedTab))
        {
            self.expected_targets.remove(pos);
        }
        let mut batch = Vec::new();
        if waiting {
            batch.push(json!({ "id": self.alloc_id(), "sessionId": sid, "method": "Runtime.runIfWaitingForDebugger", "params": {} }));
        }
        batch.push(json!({ "id": self.alloc_id(), "method": "Target.detachFromTarget", "params": { "sessionId": sid } }));
        self.pipe.send_batch(&batch);
        effects
    }

    fn handle_binding_call(&mut self, sid: &str, payload: Value) -> Vec<ExtEffect> {
        let mut effects = Vec::new();
        let Some(sess_ext_id) = self.sessions.get(sid).map(|s| s.ext_id.clone()) else {
            return effects;
        };
        let op = payload.get("op").and_then(Value::as_str).unwrap_or("");
        match op {
            "shim_ready" => {
                if let Some(sess) = self.sessions.get_mut(sid) {
                    sess.shimmed = true;
                }
                if let Some(cid) = self.active_chromium_tab_id_for(&sess_ext_id) {
                    self.push_to_session(sid, &json!({ "kind": "active_tab", "tabId": cid }));
                }
                self.trigger_tab_id_probe_for(&sess_ext_id);
            }
            "subscribe" => {
                let Some(name) = payload.get("name").and_then(Value::as_str) else {
                    return effects;
                };
                // Exact allow-list of supported events (prevents unbounded `subscribers` OOM).
                if !rpc::is_supported_event(name) {
                    return effects;
                }
                let allowed = self
                    .extensions
                    .get(&sess_ext_id)
                    .is_some_and(|e| rpc::is_event_allowed(&e.manifest, name));
                if !allowed {
                    return effects;
                }
                let listen = payload.get("listen").and_then(Value::as_bool).unwrap_or(true);
                let is_sw = self.sessions.get(sid).is_some_and(|s| s.kind == SessionKind::ServiceWorker);
                let ext_version = self
                    .is_loaded_extension(&sess_ext_id)
                    .then(|| self.extensions.get(&sess_ext_id).map(|e| e.manifest.version.clone()))
                    .flatten();
                if listen {
                    self.subscribers
                        .entry(name.to_owned())
                        .or_default()
                        .entry(sess_ext_id.clone())
                        .or_default()
                        .insert(sid.to_owned());
                    if name == "tabs.onActivated" {
                        self.announce_active_tab_for(&sess_ext_id);
                    }
                    if is_sw
                        && super::sw_wake::is_wakeable_event(name)
                        && let Some(ref version) = ext_version
                    {
                        self.sw_wake.record_sw_listener(&sess_ext_id, version, name, true);
                        // Atomically drain and deliver any queued wake events for this event
                        // strictly once to the woken service worker session.
                        for (ev_name, ev_args) in self.sw_wake.take_matching(&sess_ext_id, name) {
                            let msg = json!({ "kind": "event", "name": ev_name, "args": ev_args });
                            self.push_to_session(sid, &msg);
                        }
                    }
                } else if let Some(by_ext) = self.subscribers.get_mut(name)
                    && let Some(set) = by_ext.get_mut(&sess_ext_id)
                {
                    set.remove(sid);
                    let any_sw_left = set.iter().any(|other| {
                        self.sessions.get(other).is_some_and(|s| s.kind == SessionKind::ServiceWorker)
                    });
                    if is_sw
                        && !any_sw_left
                        && super::sw_wake::is_wakeable_event(name)
                        && let Some(ref version) = ext_version
                    {
                        self.sw_wake.record_sw_listener(&sess_ext_id, version, name, false);
                    }
                    if set.is_empty() {
                        by_ext.remove(&sess_ext_id);
                    }
                    if by_ext.is_empty() {
                        self.subscribers.remove(name);
                    }
                }
            }
            "popup_size" => {
                if self.sessions.get(sid).is_some_and(|s| s.kind == SessionKind::PopupPage)
                    && let (Some(w), Some(h)) = (
                        payload.get("width").and_then(Value::as_u64),
                        payload.get("height").and_then(Value::as_u64),
                    )
                {
                    let clamped = (w.clamp(25, 800) as u32, h.clamp(25, 600) as u32);
                    let changed = self
                        .sessions
                        .get(sid)
                        .is_none_or(|s| s.last_popup_size != Some(clamped));
                    if changed && self.allow_popup_resize(&sess_ext_id) {
                        if let Some(s) = self.sessions.get_mut(sid) {
                            s.last_popup_size = Some(clamped);
                        }
                        effects.push(ExtEffect::PopupResized {
                            ext_id: sess_ext_id,
                            width: clamped.0,
                            height: clamped.1,
                        });
                    }
                }
            }
            "self_tab" => {
                // Binding an extension page's own Chromium tab ID (`tabs.ts:345-349`); strictly
                // restricted to `ExtensionPage` sessions and re-validated against `visible_tabs` +
                // `extension_id_from_url == ext_id` in `rpc.rs` so no page can forge another tab.
                if let Some(cid_i64) = payload.get("id").and_then(Value::as_i64)
                    && cid_i64 > 0
                    && cid_i64 < (i32::MAX - 1024) as i64
                    && let Some(sess) = self.sessions.get_mut(sid)
                    && sess.kind == SessionKind::ExtensionPage
                {
                    sess.chromium_tab_id = Some(cid_i64 as i32);
                }
            }
            "tab_seen" => {
                // Scoped strictly to `sess_ext_id` and bounded around `seed_session_id` so one
                // extension can never poison another extension's tab IDs (`review-rd294.md` #6).
                if let (Some(cid_i64), Some(url)) = (
                    payload.get("id").and_then(Value::as_i64),
                    payload.get("url").and_then(Value::as_str),
                ) && cid_i64 > 0
                    && cid_i64 < (i32::MAX - 1024) as i64
                {
                    self.reconcile_chromium_tab_id(&sess_ext_id, cid_i64 as i32, url);
                }
            }
            "probe_tabs_reply" => {
                // Strictly require matching `ext_id`, `probe_id`, AND `sid` against `probe_in_flight`.
                let probe_id = payload.get("probeId").and_then(Value::as_i64);
                let matched = self
                    .probe_in_flight
                    .get(&sess_ext_id)
                    .is_some_and(|p| Some(p.probe_id) == probe_id && p.sid == sid);
                if !matched {
                    return effects;
                }
                let in_flight = self.probe_in_flight.remove(&sess_ext_id).unwrap();
                if let Some(found) = payload.get("found").and_then(Value::as_array) {
                    self.apply_probed_tabs_for(&sess_ext_id, found, &in_flight.candidates);
                }
                effects.extend(self.flush_pending_rpcs_for(&sess_ext_id));
            }
            "call" => {
                self.metrics.rpc_calls += 1;
                let req_id = payload.get("reqId").and_then(Value::as_i64).unwrap_or(0);
                let name = payload.get("name").and_then(Value::as_str).unwrap_or("").to_owned();
                let args = payload.get("args").and_then(Value::as_array).cloned().unwrap_or_default();
                let needs_tab_ids = matches!(
                    name.as_str(),
                    "tabs.query"
                        | "tabs.get"
                        | "tabs.getCurrent"
                        | "windows.get"
                        | "windows.getCurrent"
                        | "windows.getLastFocused"
                        | "windows.getAll"
                );
                if needs_tab_ids
                    && self.has_unbound_visible_tab_for(&sess_ext_id)
                    && self.seed_session_id.is_some()
                    && self.pending_rpcs.len() < MAX_PENDING_RPCS
                {
                    self.pending_rpcs.push(PendingRpc {
                        session_id: sid.to_owned(),
                        ext_id: sess_ext_id.clone(),
                        req_id,
                        name,
                        args,
                    });
                    self.trigger_tab_id_probe_for(&sess_ext_id);
                    return effects;
                }
                effects.extend(self.dispatch_rpc(sid, &sess_ext_id, req_id, &name, &args));
            }
            _ => {}
        }
        effects
    }

    pub(super) fn maybe_start_seed_probe(&mut self) -> Vec<ExtEffect> {
        if self.seed_probe_started || self.seed_session_id.is_some() {
            return Vec::new();
        }
        let Some(first_ext_id) = self
            .extensions
            .iter()
            .find(|(_, e)| e.loaded)
            .map(|(id, _)| id.clone())
        else {
            return Vec::new();
        };
        self.seed_probe_started = true;
        self.expected_targets.push_back(ExpectedTarget::SeedProbe { ext_id: first_ext_id.clone() });
        vec![ExtEffect::OpenSeedProbe {
            url: format!("chrome-extension://{first_ext_id}/manifest.json"),
        }]
    }

    pub(super) fn trigger_tab_id_probes_all(&mut self) {
        let ext_ids: Vec<String> = self.extensions.keys().cloned().collect();
        for ext_id in ext_ids {
            self.trigger_tab_id_probe_for(&ext_id);
        }
    }

    pub(super) fn trigger_tab_id_probe_for(&mut self, ext_id: &str) {
        if self.probe_in_flight.contains_key(ext_id) {
            return;
        }
        let Some(seed) = self.seed_session_id else {
            return;
        };
        if !self.has_unbound_visible_tab_for(ext_id) {
            return;
        }
        // Strictly probe using a shimmed session belonging to `ext_id` itself so a low-privilege
        // extension can never supply or poison another extension's `chromium_tab_id` map.
        let Some(sid) = self
            .sessions
            .iter()
            .find(|(id, s)| {
                s.ext_id == ext_id
                    && s.shimmed
                    && !matches!(s.kind, SessionKind::SeedProbe | SessionKind::FileGuardOnly)
                    && !self.failed_probe_sessions.contains(*id)
            })
            .map(|(sid, _)| sid.clone())
        else {
            return;
        };
        // Bounded candidate window (<= 49 IDs, saturating arithmetic — zero overflow / OOM).
        let base = self.ext_last_known_session_id.get(ext_id).copied().unwrap_or(0).max(seed);
        let lo = base.saturating_sub(16).max(1);
        let hi = base.saturating_add(32);
        let candidates: Vec<i32> = (lo..=hi).collect();
        let probe_id = self.next_probe_id;
        self.next_probe_id += 1;
        self.probe_in_flight.insert(
            ext_id.to_owned(),
            ProbeInFlight {
                probe_id,
                sid: sid.clone(),
                sent_at: Instant::now(),
                candidates: candidates.clone(),
            },
        );
        self.push_to_session(
            &sid,
            &json!({ "kind": "probe_tabs", "probeId": probe_id, "candidates": candidates }),
        );
    }

    fn apply_probed_tabs_for(&mut self, ext_id: &str, found: &[Value], allowed_candidates: &[i32]) {
        let already_used: HashSet<i32> = self
            .ext_tab_ids
            .get(ext_id)
            .map(|m| m.values().copied().collect())
            .unwrap_or_default();
        let mut available: Vec<(i32, String)> = found
            .iter()
            .take(64)
            .filter_map(|v| {
                let id_i64 = v.get("id")?.as_i64()?;
                let id = i32::try_from(id_i64).ok()?;
                if !allowed_candidates.contains(&id) || already_used.contains(&id) {
                    return None;
                }
                let url = v.get("url").and_then(Value::as_str).unwrap_or("").to_owned();
                if url.is_empty() || url.starts_with("chrome-extension://") {
                    return None;
                }
                Some((id, url))
            })
            .collect();
        let map = self.ext_tab_ids.entry(ext_id.to_owned()).or_default();
        let last_known = self.ext_last_known_session_id.entry(ext_id.to_owned()).or_insert(0);
        for tab in &self.tabs {
            if tab.agent_driven || tab.lifecycle.is_discarded() || map.contains_key(&tab.engine_tab_id) {
                continue;
            }
            if let Some(pos) = available.iter().position(|(_, u)| !u.is_empty() && u == &tab.url) {
                let (cid, _) = available.remove(pos);
                map.insert(tab.engine_tab_id, cid);
                *last_known = (*last_known).max(cid);
            }
        }
        // Only fall back to positional matching when there is unambiguously a single remaining
        // unbound visible tab and a single remaining candidate ID.
        let remaining_unbound: Vec<u64> = self
            .tabs
            .iter()
            .filter(|t| !t.agent_driven && !t.lifecycle.is_discarded() && !map.contains_key(&t.engine_tab_id))
            .map(|t| t.engine_tab_id)
            .collect();
        if remaining_unbound.len() == 1 && available.len() == 1 {
            let (cid, _) = available.remove(0);
            map.insert(remaining_unbound[0], cid);
            *last_known = (*last_known).max(cid);
        }
        if let Some(cid) = self.active_chromium_tab_id_for(ext_id) {
            self.broadcast_active_tab_for(ext_id, cid);
        }
        self.announce_active_tab_for(ext_id);
    }

    fn reconcile_chromium_tab_id(&mut self, ext_id: &str, cid: i32, url: &str) {
        let Some(seed) = self.seed_session_id else {
            return;
        };
        let prev_last = self.ext_last_known_session_id.get(ext_id).copied().unwrap_or(0);
        let min_ok = seed.saturating_sub(16).max(1);
        let max_ok = prev_last.max(seed).saturating_add(64);
        if cid < min_ok || cid > max_ok {
            return;
        }
        let map = self.ext_tab_ids.entry(ext_id.to_owned()).or_default();
        if map.values().any(|&v| v == cid) {
            return;
        }
        if let Some(tab) = self.tabs.iter().find(|t| {
            !t.agent_driven
                && !t.lifecycle.is_discarded()
                && !map.contains_key(&t.engine_tab_id)
                && (t.url == url || t.url == "about:blank")
        }) {
            map.insert(tab.engine_tab_id, cid);
            let last_known = self.ext_last_known_session_id.entry(ext_id.to_owned()).or_insert(0);
            *last_known = (*last_known).max(cid);
            self.announce_active_tab_for(ext_id);
        }
    }

    fn flush_pending_rpcs_for(&mut self, ext_id: &str) -> Vec<ExtEffect> {
        let mut ready = Vec::new();
        let mut keep = Vec::new();
        for rpc in std::mem::take(&mut self.pending_rpcs) {
            if rpc.ext_id == ext_id {
                ready.push(rpc);
            } else {
                keep.push(rpc);
            }
        }
        self.pending_rpcs = keep;
        let mut effects = Vec::new();
        for rpc in ready {
            effects.extend(self.dispatch_rpc(&rpc.session_id, &rpc.ext_id, rpc.req_id, &rpc.name, &rpc.args));
        }
        effects
    }
    fn resume_confirmed_session(&mut self, sid: &str, should_resume: bool) {
        let sw_tid = self
            .sessions
            .get(sid)
            .filter(|s| s.kind == SessionKind::ServiceWorker && !s.target_id.is_empty())
            .map(|s| s.target_id.clone());
        if should_resume {
            let _ = self.send_cdp("Runtime.runIfWaitingForDebugger", json!({}), Some(sid));
        }
        // Now that BOTH `Fetch.enable` + `scriptFirstStatement` AND post-load verification
        // (`ext.loaded == true`) have confirmed, safely release and detach any duplicate SW
        // sessions held for this target so Chromium's waitForDebugger throttle never deadlocks.
        if let Some(tid) = sw_tid
            && let Some(dups) = self.deferred_dup_sw_detaches.remove(&tid)
        {
            let mut batch = Vec::new();
            for (dup_sid, dup_waiting) in dups {
                if dup_waiting {
                    batch.push(json!({ "id": self.alloc_id(), "sessionId": dup_sid, "method": "Runtime.runIfWaitingForDebugger", "params": {} }));
                }
                batch.push(json!({ "id": self.alloc_id(), "method": "Target.detachFromTarget", "params": { "sessionId": dup_sid } }));
            }
            self.pipe.send_batch(&batch);
        }
    }

    fn resume_sessions_loaded_for(&mut self, ext_id: &str) {
        let mut remaining = Vec::new();
        let mut to_resume = Vec::new();
        for (sid, should_resume) in std::mem::take(&mut self.pending_loaded_resume) {
            if self.sessions.get(&sid).is_some_and(|s| s.ext_id == ext_id) {
                to_resume.push((sid, should_resume));
            } else if self.sessions.contains_key(&sid) {
                remaining.push((sid, should_resume));
            }
        }
        self.pending_loaded_resume = remaining;
        for (sid, should_resume) in to_resume {
            self.resume_confirmed_session(&sid, should_resume);
        }
    }

    fn drop_loaded_resumes_for(&mut self, ext_id: &str) {
        self.pending_loaded_resume
            .retain(|(sid, _)| self.sessions.get(sid).is_some_and(|s| s.ext_id != ext_id));
    }
}

const IFRAME_LOCKDOWN_JS: &str = r#"(() => {
  const allowed = new Set(["runtime", "i18n", "storage", "dom"]);
  const alwaysLock = [
    "tabs", "windows", "action", "browserAction", "pageAction", "contextMenus", "webNavigation",
    "downloads", "management", "notifications", "permissions", "history", "bookmarks", "sessions",
    "privacy", "fontSettings", "identity", "sidePanel", "search", "contentSettings",
    "desktopCapture", "tabCapture", "pageCapture", "webAuthenticationProxy", "browsingData",
    "system", "userScripts", "tts", "ttsEngine", "debugger", "proxy", "topSites", "readingList",
    "tabGroups", "omnibox", "devtools", "extension", "commands", "webRequest",
    "declarativeNetRequest", "scripting", "offscreen", "cookies", "alarms", "idle", "power",
    "app", "csi", "loadTimes", "gcm", "instanceID", "declarativeContent"
  ];
  const lockProp = (obj, prop) => {
    try { Object.defineProperty(obj, prop, { value: undefined, configurable: false, enumerable: false, writable: false }); } catch {}
  };
  for (const rootName of ["chrome", "browser"]) {
    let root;
    try { root = globalThis[rootName]; } catch { continue; }
    if (!root || typeof root !== "object") continue;
    let keys = [];
    try { keys = Object.getOwnPropertyNames(root); } catch {}
    for (const k of keys) {
      if (!allowed.has(k)) lockProp(root, k);
    }
    for (const api of alwaysLock) lockProp(root, api);
    try {
      const rt = root.runtime;
      if (rt && typeof rt === "object") {
        lockProp(rt, "connectNative");
        lockProp(rt, "sendNativeMessage");
      }
    } catch {}
  }
})();"#;

fn push_bounded(vec: &mut Vec<u64>, val: u64) {
    if vec.len() < MAX_METRIC_SAMPLES {
        vec.push(val);
    }
}

fn fetch_enable_cmd(id: i64, sid: &str) -> Value {
    json!({
        "id": id,
        "sessionId": sid,
        "method": "Fetch.enable",
        "params": {
            "patterns": [
                { "urlPattern": "file://*" },
                { "urlPattern": "blob:file://*" },
                { "urlPattern": "filesystem:file://*" }
            ]
        }
    })
}

fn auto_attach_cmd(id: i64, sid: &str) -> Value {
    json!({
        "id": id,
        "sessionId": sid,
        "method": "Target.setAutoAttach",
        "params": {
            "autoAttach": true,
            "waitForDebuggerOnStart": true,
            "flatten": true
        }
    })
}
