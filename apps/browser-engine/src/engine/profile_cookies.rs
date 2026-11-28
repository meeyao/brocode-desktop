//! Profile cookie messages (`UP-095` Phase 3b): `CefCookieManager` list/import/delete, the startup
//! persistence probe, "Sign out everywhere", and the two-way remote cookie mirror.

use serde_json::json;

use std::sync::atomic::Ordering;

use super::EngineManager;
use super::sign_in_gate::HeldNav;
use cef::{CefString, ImplBrowser, ImplBrowserHost, ImplFrame, ImplRequestContext};

use crate::wire::{EngineMsg, HostMsg};

impl EngineManager {
    pub(super) fn handle_profile_cookies(&mut self, msg: HostMsg) {
        match msg {
            HostMsg::ProfileCookiesList { request_id, partition } => {
                self.cookies.list(&self.contexts, partition.as_deref(), request_id, self.out_tx.clone());
            }
            HostMsg::ProfileCookiesSet { request_id, partition, cookies } => {
                self.cookies.set_batch(&self.contexts, partition.as_deref(), cookies, request_id, self.out_tx.clone());
            }
            HostMsg::ProfileCookieDelete { request_id, partition, url, name } => {
                self.cookies.delete_one(&self.contexts, partition.as_deref(), &url, &name, request_id, self.out_tx.clone());
            }
            HostMsg::ProfileProbeCookies { request_id } => {
                self.cookies.probe_persistence(&self.profile_cache, request_id, self.out_tx.clone());
            }
            HostMsg::ClearProfileData { request_id, sites } => {
                for t in self.tabs.values() {
                    if !matches!(
                        t.context,
                        crate::wire::ContextKind::Default | crate::wire::ContextKind::RemoteServer { .. }
                    ) {
                        continue;
                    }
                    let url = t.snap.lock().unwrap().url.clone();
                    if let Some(origin) = browser_policy::destination_origin(&url) {
                        self.send_internal_cdp(
                            &t.host,
                            "Storage.clearDataForOrigin",
                            json!({ "origin": origin, "storageTypes": "all" }),
                        );
                    }
                    self.send_internal_cdp(&t.host, "Network.clearBrowserCache", json!({}));
                }
                self.clear_every_site(request_id, sites);
                self.dispatch_sign_in_loads();
            }
            HostMsg::SetShareSignIns { enabled } => {
                self.cookies.set_share_sign_ins(enabled, &self.contexts);
                self.dispatch_sign_in_loads();
            }
            HostMsg::SyncRemoteCookies { request_id, partition, from_remote } => {
                let side = if from_remote {
                    crate::cookies::LinkSide::Other
                } else {
                    crate::cookies::LinkSide::Profile
                };
                self.cookies.sync_partition_now(&self.contexts, &partition, side, request_id, self.out_tx.clone());
            }
            _ => {}
        }
    }

    /// "Clear browsing data" for every site, not only the open ones (official `profile/index.ts:523-540`):
    /// through a hidden page in each context (`clear_storage`), every site's stored data in the shared
    /// profile, in each remote server's partition and in each open preview's; the HTTP cache of
    /// each; and every cookie and HTTP sign-in (`CookieStore::clear_all`). `ProfileActionDone`
    /// comes once all of them have ended, `ok` only when each did and nothing of the person's is found
    /// left: no cookie but the engine's own probe in the shared profile or a remote partition, and no
    /// bytes in a cleared storage type for any of `sites` (the app's list) or the open tabs' sites.
    fn clear_every_site(&mut self, request_id: u64, mut sites: Vec<String>) {
        use crate::cookies::JoinPart;
        let out = self.out_tx.clone();
        for t in self.tabs.values() {
            if matches!(t.context, crate::wire::ContextKind::Default)
                && let Some(origin) = browser_policy::destination_origin(&t.snap.lock().unwrap().url)
            {
                sites.push(origin);
            }
        }
        sites.retain(|s| s.starts_with("https://") || s.starts_with("http://"));
        sites.sort();
        sites.dedup();
        let keep_workers = self.extensions.as_ref().is_some_and(|e| e.installed_count() > 0);
        // Remote partitions (all pooled ones, tab or not) and the open previews' contexts, each once.
        let mut isolated: Vec<cef::RequestContext> = self.contexts.remote_partitions().into_iter().map(|(_, ctx)| ctx).collect();
        for t in self.tabs.values() {
            if !matches!(t.context, crate::wire::ContextKind::Preview { .. } | crate::wire::ContextKind::Artifact { .. }) {
                continue;
            }
            if let Some(mut ctx) = t.host.request_context()
                && !isolated.iter().any(|c| c.is_same(Some(&mut ctx)) != 0)
            {
                isolated.push(ctx);
            }
        }
        let mut managers: Vec<cef::CookieManager> = cef::cookie_manager_get_global_manager(None).into_iter().collect();
        managers.extend(self.contexts.remote_partitions().into_iter().filter_map(|(_, ctx)| ctx.cookie_manager(None)));
        let global = cef::request_context_get_global_context();
        // Each isolated context's storage and cache, the shared profile's storage and cache, and the cookies.
        let join = crate::cookies::Join::new(2 * isolated.len() + 3, Box::new(move |ok| {
            // Then the cookies are counted again: any but the probe left fails the clear.
            let left = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
            let counted = crate::cookies::Join::new(managers.len().max(1), {
                let left = left.clone();
                Box::new(move |all| {
                    let ok = ok && all && left.load(std::sync::atomic::Ordering::Acquire) == 0;
                    let _ = out.send(EngineMsg::ProfileActionDone { request_id, ok });
                })
            });
            if managers.is_empty() {
                return counted.part()(true);
            }
            for mgr in &managers {
                let part = counted.part();
                let left = left.clone();
                crate::cookies::visit_all(mgr, false, move |cookies| {
                    left.fetch_add(cookies.len(), std::sync::atomic::Ordering::AcqRel);
                    part(true);
                });
            }
        }));
        for ctx in isolated {
            let cache = join.part();
            let mut cb = crate::cookies::completion_cb(move || cache(true));
            ctx.clear_http_cache(Some(&mut cb));
            let base = self.next_cdp_id.fetch_add(crate::clear_storage::ids_for(0), std::sync::atomic::Ordering::Relaxed);
            // Remote and preview partitions hold no extension.
            crate::clear_storage::clear_context(Some(ctx), false, Vec::new(), base, join.part());
        }
        let cache = join.part();
        match &global {
            Some(global) => {
                let mut cb = crate::cookies::completion_cb(move || cache(true));
                global.clear_http_cache(Some(&mut cb));
            }
            None => cache(false),
        }
        let base = self.next_cdp_id.fetch_add(crate::clear_storage::ids_for(sites.len()), std::sync::atomic::Ordering::Relaxed);
        crate::clear_storage::clear_context(None, keep_workers, sites, base, join.part());
        self.cookies.clear_all(&self.contexts, join.part());
    }

    /// A finished page load may have changed cookies: marks its side of the mirror dirty, then
    /// runs the mirror's periodic reconcile.
    pub(super) fn poll_cookie_mirror(&mut self) {
        let mut profile_dirty = false;
        let mut remote_dirty: Vec<String> = Vec::new();
        for tab in self.tabs.values_mut() {
            let loading = tab.snap.lock().unwrap().loading;
            if tab.last_loading && !loading {
                match &tab.context {
                    crate::wire::ContextKind::Default => profile_dirty = true,
                    crate::wire::ContextKind::RemoteServer { server_id, .. } => remote_dirty.push(server_id.clone()),
                    crate::wire::ContextKind::Preview { .. }
                    | crate::wire::ContextKind::Artifact { .. }
                    | crate::wire::ContextKind::Private => {}
                }
            }
            tab.last_loading = loading;
        }
        if profile_dirty {
            self.cookies.on_profile_dirty(&self.contexts);
        }
        for partition in remote_dirty {
            self.cookies.on_partition_dirty(&self.contexts, &partition, crate::cookies::LinkSide::Other);
        }
        self.cookies.poll(&self.contexts);
        self.dispatch_sign_in_loads();
    }

    /// A newly opened remote tab starts (or joins) the partition's initial profile -> remote
    /// sign-in copy when sharing is on (`UP-151`, official `profile/index.ts:link`).
    pub(super) fn seed_remote_partition(&mut self, context: &crate::wire::ContextKind) {
        if let crate::wire::ContextKind::RemoteServer { server_id, .. } = context {
            self.cookies.ensure_initial_copy(&self.contexts, server_id);
        }
    }

    /// Gates an already-initialized remote tab's initial URL during `open_tab` while its
    /// partition's initial sign-in copy is still in flight.
    pub(super) fn gate_remote_open(
        &mut self,
        context: &crate::wire::ContextKind,
        tab_id: u64,
        generation: u64,
        url: &str,
    ) -> bool {
        let crate::wire::ContextKind::RemoteServer { server_id, .. } = context else {
            return false;
        };
        self.cookies.gate_navigation(
            server_id,
            HeldNav {
                tab_id,
                generation,
                url: url.to_owned(),
            },
        )
    }

    /// Gates a live remote tab's navigation while its partition's initial sign-in copy is in
    /// flight; non-HTTP URLs bypass the gate and cancel any prior held HTTP load for `tab_id`.
    pub(super) fn gate_remote_sign_in_nav(&mut self, tab_id: u64, url: &str) -> bool {
        let Some((server_id, generation)) = self.tabs.get(&tab_id).and_then(|t| match &t.context {
            crate::wire::ContextKind::RemoteServer { server_id, .. } => {
                Some((server_id.clone(), t.generation))
            }
            _ => None,
        }) else {
            return false;
        };
        let held = self.cookies.gate_navigation(
            &server_id,
            HeldNav {
                tab_id,
                generation,
                url: url.to_owned(),
            },
        );
        self.dispatch_sign_in_loads();
        held
    }

    /// Cancels a tab's queued sign-in navigation (leaving the partition copy running); when
    /// `clear_loading` is true (Stop), also clears `loading` on the snapshot and emits `TabUpdate`.
    pub(super) fn cancel_tab_sign_in_gate(&mut self, tab_id: u64, clear_loading: bool) {
        if !self.cookies.cancel_tab_gate(tab_id) {
            return;
        }
        if let Some(t) = self.tabs.get(&tab_id) {
            t.deferred_initial.store(false, Ordering::Release);
            if clear_loading {
                let mut s = t.snap.lock().unwrap();
                if s.loading {
                    s.loading = false;
                    let _ = self.out_tx.send(EngineMsg::TabUpdate { tab: s.clone() });
                }
            }
        }
    }

    /// Marks `tab_id` as waiting for a held sign-in navigation (keeping the spinner active).
    pub(super) fn mark_tab_waiting_sign_in(&self, tab_id: u64) {
        if let Some(t) = self.tabs.get(&tab_id) {
            t.deferred_initial.store(true, Ordering::Release);
            let mut s = t.snap.lock().unwrap();
            if !s.loading {
                s.loading = true;
                let _ = self.out_tx.send(EngineMsg::TabUpdate { tab: s.clone() });
            }
        }
    }

    /// Issues all navigations released by settled or timed-out partition sign-in gates.
    pub(super) fn dispatch_sign_in_loads(&mut self) {
        for nav in self.cookies.take_released_loads() {
            self.load_released_tab_url(nav.tab_id, nav.generation, nav.url);
        }
    }

    /// Loads `url` on `tab_id` once both its isolated context and any initial remote sign-in copy
    /// have settled, preserving `TabClipboardGate` ordering for agent-driven tabs.
    pub(super) fn load_released_tab_url(&mut self, tab_id: u64, generation: u64, url: String) {
        if let Some(tab) = self.tabs.get(&tab_id)
            && tab.generation == generation
            && self.contexts.is_tab_ready(tab_id)
        {
            tab.deferred_initial.store(false, Ordering::Release);
            if !url.trim().eq_ignore_ascii_case("about:blank")
                && !tab
                    .clipboard_gate
                    .hold_until_root_armed(tab.prompts.agent_driven.load(Ordering::Relaxed), &url)
                && let Some(frame) = tab.browser.main_frame()
            {
                tab.prompts.set_pending_top_url(Some(url.clone()));
                frame.load_url(Some(&CefString::from(url.as_str())));
            }
        }
    }

    /// Releases `tab_id` from its pooled context and cancels any held sign-in navigation; when
    /// that was a remote partition's last tab, the cookie mirror for that partition stops.
    pub(super) fn release_tab_context(&mut self, tab_id: u64) {
        self.cookies.cancel_tab_gate(tab_id);
        if let Some(crate::context::ContextKey::RemoteServer(id)) = self.contexts.release_tab(tab_id) {
            self.cookies.on_partition_dropped(&id);
        }
    }
}

