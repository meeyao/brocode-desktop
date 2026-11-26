//! Profiling and audit executors for `experimental.browser` v4 (`trace.start`/`stop`/`analyze`,
//! `cpu.start`/`stop`/`analyze`, `heap.snapshot`/`summary`/`query`/`object`/`compare`, and
//! truthful `lighthouse` `unsupported` refusal).

use std::time::Duration;

use opencode_kit::browser_rpc::{Command, Outcome, TargetMeta, WireFile, failure_code, new_file_id};
use rune::time::Instant;
use serde_json::{Value, json};

use super::super::session::{CapturedFile, CpuRecording, SessionAttachment, TraceRecording};
pub(super) use super::profile_analyze::stop_target;
use super::profile_analyze::{
    RESOURCES_OVERFLOW_MSG, authorized_matches, exec_cpu_analyze, exec_heap_compare,
    exec_heap_object, exec_heap_query, exec_heap_summary, exec_trace_analyze, finalize_cpu,
    finalize_trace, gzip_bytes, wire_file_from_capture,
};
use super::{ExecPhase, StepOutcome, cdp_err_msg, check_page_guards};
use crate::App;

pub(super) struct ProfileState {
    pub host_id: u64,
    pub stage: ProfileStage,
}

pub(super) enum ProfileStage {
    TraceStart { cdp_req: u64 },
    TraceStop { cdp_req: u64, ended: bool, authorized: Option<TargetMeta> },
    CpuEnable { cdp_req: u64 },
    CpuStart { cdp_req: u64 },
    CpuStop { cdp_req: u64, authorized: Option<TargetMeta> },
    HeapEnable { cdp_req: u64 },
    HeapTake { cdp_req: u64 },
}

impl ProfileState {
    pub fn cancel_cleanup(&mut self, att: &mut SessionAttachment, app: &mut App) {
        app.workspace_panes.browser.host.cancel_cdp(self.stage.cdp_req());
        match self.stage {
            ProfileStage::TraceStart { .. } | ProfileStage::TraceStop { .. } => {
                if att.trace_recording.as_ref().is_some_and(|r| r.host_id == self.host_id) {
                    att.trace_recording = None;
                }
                abort_trace(app, self.host_id);
            }
            ProfileStage::CpuEnable { .. } | ProfileStage::CpuStart { .. } | ProfileStage::CpuStop { .. } => {
                att.cpu_started.remove(&self.host_id);
                if app.workspace_panes.browser.host.tab_open_in_engine(self.host_id) {
                    let _ = app.workspace_panes.browser.host.send_typed_cdp(self.host_id, "Profiler.stop", json!({}), None);
                    let _ = app.workspace_panes.browser.host.send_typed_cdp(self.host_id, "Profiler.disable", json!({}), None);
                }
            }
            ProfileStage::HeapEnable { .. } | ProfileStage::HeapTake { .. } => {
                att.heap_chunks.remove(&self.host_id);
                if app.workspace_panes.browser.host.tab_open_in_engine(self.host_id) {
                    let _ = app.workspace_panes.browser.host.send_typed_cdp(self.host_id, "HeapProfiler.disable", json!({}), None);
                }
            }
        }
    }
}

pub(in super::super) fn clear_trace_owner(app: &mut App, host_id: u64) {
    let st = app.kernel.slots.get_mut(&super::super::BROWSER_RPC);
    if st.active_trace_host == Some(host_id) {
        st.active_trace_host = None;
    }
}

/// Sends `Tracing.end` (when the tab is still open in the engine) and releases the app-wide trace
/// owner lock so tab close, session disposal, or command cancel never wedges Chromium's tracing.
pub(in super::super) fn abort_trace(app: &mut App, host_id: u64) {
    clear_trace_owner(app, host_id);
    if app.workspace_panes.browser.host.tab_open_in_engine(host_id) {
        let _ = app.workspace_panes.browser.host.send_typed_cdp(host_id, "Tracing.end", json!({}), None);
    }
}

impl ProfileStage {
    pub fn cdp_req(&self) -> u64 {
        match self {
            Self::TraceStart { cdp_req }
            | Self::TraceStop { cdp_req, .. }
            | Self::CpuEnable { cdp_req }
            | Self::CpuStart { cdp_req }
            | Self::CpuStop { cdp_req, .. }
            | Self::HeapEnable { cdp_req }
            | Self::HeapTake { cdp_req } => *cdp_req,
        }
    }
}

const MAX_TRACE_EVENTS: usize = 250_000;
const MAX_HEAP_RAW_BYTES: usize = 64 * 1024 * 1024;
const TRACE_DEFAULT_DURATION_MS: u64 = 10_000;
const TRACE_STOP_TIMEOUT_MS: u64 = 10_000;
const CPU_MAX_DURATION_MS: u64 = 30_000;
const CPU_STOP_TIMEOUT_MS: u64 = 10_000;

const TRACE_FLUSH_TIMEOUT_MSG: &str = "Chromium did not finish flushing the trace within 10 seconds. No complete export is confirmed. Check browser.files.list({tabID}); do not start another recording until the current trace has finished.";
const CPU_STOP_TIMEOUT_MSG: &str = "Chromium did not stop the CPU profile within 10 seconds. No profile export is confirmed. Check browser.files.list({tabID}) before starting another profile.";
const NAVIGATED_DURING_STOP_MSG: &str = "The page or its frames navigated while stopping the recording. Retry so permissions can re-check all visited URLs.";

/// Routes navigation URLs, trace data, and heap snapshot chunks from tab CDP events into active
/// recording buffers (`profiling.ts:221-223, 295`).
pub(in super::super) fn ingest_cdp_event(att: &mut SessionAttachment, ev: &browser_pane::CdpEvent) {
    if ev.method == "Page.frameNavigated"
        && let Some(url) = ev.params.get("frame").and_then(|f| f.get("url")).and_then(Value::as_str)
    {
        att.record_recording_url(ev.tab_id, url);
        return;
    }
    if ev.method == "Page.navigatedWithinDocument"
        && let Some(url) = ev.params.get("url").and_then(Value::as_str)
    {
        att.record_recording_url(ev.tab_id, url);
        return;
    }
    if matches!(ev.method.as_str(), "Target.attachedToTarget" | "Target.targetInfoChanged")
        && let Some(url) = ev.params.get("targetInfo").and_then(|t| t.get("url")).and_then(Value::as_str)
    {
        att.record_recording_url(ev.tab_id, url);
        return;
    }
    if ev.method == "Tracing.dataCollected"
        && let Some(rec) = att.trace_recording.as_mut()
        && rec.host_id == ev.tab_id
        && let Some(pid) = ev.params.get("rendererPid").and_then(Value::as_u64).filter(|&p| p > 0)
        && let Some(arr) = ev.params.get("value").and_then(Value::as_array)
    {
        let locked = *rec.renderer_pid.get_or_insert(pid);
        if locked != pid {
            return;
        }
        for e in arr.iter().filter(|e| e.get("pid").and_then(Value::as_u64) == Some(locked)) {
            if rec.events.len() >= MAX_TRACE_EVENTS {
                rec.data_loss = true;
                break;
            }
            rec.events.push(e.clone());
        }
    } else if ev.method == "Tracing.tracingComplete"
        && let Some(rec) = att.trace_recording.as_mut()
        && rec.host_id == ev.tab_id
    {
        if let Some(pid) = ev.params.get("rendererPid").and_then(Value::as_u64).filter(|&p| p > 0) {
            rec.renderer_pid.get_or_insert(pid);
        }
        rec.data_loss |= ev.params.get("dataLossOccurred").and_then(Value::as_bool).unwrap_or(false);
        rec.complete = true;
    } else if ev.method == "HeapProfiler.addHeapSnapshotChunk"
        && let Some((buf, overflowed)) = att.heap_chunks.get_mut(&ev.tab_id)
        && let Some(chunk) = ev.params.get("chunk").and_then(Value::as_str)
    {
        if buf.len().saturating_add(chunk.len()) > MAX_HEAP_RAW_BYTES {
            *overflowed = true;
            buf.clear();
        } else if !*overflowed {
            buf.push_str(chunk);
        }
    }
}

/// Drives `trace.start` (`durationMs`) and `cpu.start` (`CPU_MAX_DURATION_MS = 30 s`) auto-stop
/// timers and bounded stop/flush timeouts (`profiling-machine.ts:8-14, 200-270`).
pub(in super::super) fn poll_auto_stops(
    att: &mut SessionAttachment,
    app: &mut App,
    now: Instant,
) -> Option<Instant> {
    let mut wake: Option<Instant> = None;
    let mut note = |t: Instant| wake = Some(wake.map_or(t, |cur| cur.min(t)));

    // 1. Performance trace auto-stop.
    let mut finish_trace = false;
    if let Some(rec) = att.trace_recording.as_mut()
        && rec.auto_stopped.is_none()
    {
        if let Some(req) = rec.auto_stop_req {
            if !rec.auto_stop_ended
                && let Some(resp) = app.workspace_panes.browser.host.take_cdp_response(req)
            {
                match resp {
                    Ok(_) => rec.auto_stop_ended = true,
                    Err(e) => {
                        rec.auto_stop_req = None;
                        rec.auto_stop_deadline = None;
                        rec.auto_stopped = Some(Err(cdp_err_msg(&e)));
                        clear_trace_owner(app, rec.host_id);
                    }
                }
            }
            if rec.auto_stop_ended && rec.complete {
                finish_trace = true;
            } else if let Some(deadline) = rec.auto_stop_deadline {
                if now >= deadline {
                    app.workspace_panes.browser.host.cancel_cdp(req);
                    rec.auto_stop_req = None;
                    rec.auto_stop_deadline = None;
                    rec.auto_stopped = Some(Err(TRACE_FLUSH_TIMEOUT_MSG.to_owned()));
                    clear_trace_owner(app, rec.host_id);
                } else {
                    note(deadline);
                }
            }
        } else if let Some(stop_at) = rec.stop_at {
            if now >= stop_at {
                rec.stop_at = None;
                match app.workspace_panes.browser.host.send_typed_cdp(rec.host_id, "Tracing.end", json!({}), None) {
                    Ok(req) => {
                        let deadline = now + Duration::from_millis(TRACE_STOP_TIMEOUT_MS);
                        rec.auto_stop_req = Some(req);
                        rec.auto_stop_deadline = Some(deadline);
                        note(deadline);
                    }
                    Err(e) => {
                        rec.auto_stopped = Some(Err(e));
                        clear_trace_owner(app, rec.host_id);
                    }
                }
            } else {
                note(stop_at);
            }
        }
    }
    if finish_trace && let Some(mut rec) = att.trace_recording.take() {
        let events = std::mem::take(&mut rec.events);
        let saved = finalize_trace(att, app, &mut rec, events, now);
        clear_trace_owner(app, rec.host_id);
        rec.auto_stop_req = None;
        rec.auto_stop_deadline = None;
        rec.auto_stopped = Some(saved);
        att.trace_recording = Some(rec);
    }

    // 2. CPU profile 30 s auto-stop.
    let cpu_tabs: Vec<u64> = att.cpu_started.keys().copied().collect();
    for hid in cpu_tabs {
        let mut trigger_stop = false;
        let mut completed: Option<Result<Value, Value>> = None;
        if let Some(rec) = att.cpu_started.get_mut(&hid)
            && rec.auto_stopped.is_none()
        {
            if let Some(req) = rec.auto_stop_req {
                if let Some(resp) = app.workspace_panes.browser.host.take_cdp_response(req) {
                    rec.auto_stop_req = None;
                    rec.auto_stop_deadline = None;
                    completed = Some(resp);
                } else if let Some(deadline) = rec.auto_stop_deadline {
                    if now >= deadline {
                        app.workspace_panes.browser.host.cancel_cdp(req);
                        rec.auto_stop_req = None;
                        rec.auto_stop_deadline = None;
                        rec.auto_stopped = Some(Err(CPU_STOP_TIMEOUT_MSG.to_owned()));
                    } else {
                        note(deadline);
                    }
                }
            } else if let Some(stop_at) = rec.stop_at {
                if now >= stop_at {
                    rec.stop_at = None;
                    trigger_stop = true;
                } else {
                    note(stop_at);
                }
            }
        }
        if trigger_stop {
            let res = app.workspace_panes.browser.host.send_typed_cdp(hid, "Profiler.stop", json!({}), None);
            if let Some(rec) = att.cpu_started.get_mut(&hid) {
                match res {
                    Ok(req) => {
                        let deadline = now + Duration::from_millis(CPU_STOP_TIMEOUT_MS);
                        rec.auto_stop_req = Some(req);
                        rec.auto_stop_deadline = Some(deadline);
                        note(deadline);
                    }
                    Err(e) => rec.auto_stopped = Some(Err(e)),
                }
            }
        }
        if let Some(resp) = completed
            && let Some(mut rec) = att.cpu_started.remove(&hid)
        {
            let _ = app.workspace_panes.browser.host.send_typed_cdp(hid, "Profiler.disable", json!({}), None);
            let saved = finalize_cpu(att, app, hid, &mut rec, resp, now);
            rec.auto_stopped = Some(saved);
            att.cpu_started.insert(hid, rec);
        }
    }
    wake
}

pub(super) fn try_start(
    att: &mut SessionAttachment,
    app: &mut App,
    cmd: &Command,
    now: Instant,
) -> Option<StepOutcome> {
    match cmd.action_type()? {
        "trace.start" => Some(start_trace(att, app, cmd, now)),
        "trace.stop" => Some(stop_trace(att, app, cmd)),
        "trace.analyze" => Some(exec_trace_analyze(att, app, cmd)),
        "cpu.start" => Some(start_cpu(att, app, cmd, now)),
        "cpu.stop" => Some(stop_cpu(att, app, cmd)),
        "cpu.analyze" => Some(exec_cpu_analyze(att, app, cmd)),
        "heap.snapshot" => Some(start_heap_snapshot(att, app, cmd)),
        "heap.summary" => Some(exec_heap_summary(att, app, cmd)),
        "heap.query" => Some(exec_heap_query(att, app, cmd)),
        "heap.object" => Some(exec_heap_object(att, app, cmd)),
        "heap.compare" => Some(exec_heap_compare(att, app, cmd)),
        "lighthouse" => Some(StepOutcome::Done(Outcome::fail(
            failure_code::UNSUPPORTED,
            "browser.lighthouse is unavailable in Brocode: Node.js Lighthouse is not bundled in the native Rust desktop app. Use browser.trace.*, browser.cpu.*, browser.heap.*, and browser.snapshot instead.",
        ))),
        _ => None,
    }
}

/// Recording requires BOTH `frame_authority` (engine extensions-active profiling refusal) AND
/// `trace_isolation` (dedicated capability bit for committed-main-frame renderer PID filtering
/// and the engine-wide single-trace lock; prevents unfiltered traces on `156.0.26-4`).
fn require_profiling_engine(app: &App, name: &str) -> Result<(), Outcome> {
    let host = &app.workspace_panes.browser.host;
    if !host.engine_frame_authority() || !host.engine_trace_isolation() {
        return Err(Outcome::fail(
            failure_code::UNSUPPORTED,
            format!("browser.{name} requires an updated browser engine."),
        ));
    }
    Ok(())
}

fn start_trace(att: &mut SessionAttachment, app: &mut App, cmd: &Command, now: Instant) -> StepOutcome {
    let Some(wire_id) = cmd.tab_id() else {
        return StepOutcome::Done(Outcome::fail(failure_code::OPERATION_FAILED, "Missing tabID."));
    };
    let host_id = match att.resolve_tab(app, wire_id) {
        Ok(id) => id,
        Err(msg) => return StepOutcome::Done(Outcome::fail(failure_code::OPERATION_FAILED, msg)),
    };
    if let Err(o) = check_page_guards(app, host_id, "trace.start") {
        return StepOutcome::Done(o);
    }
    if let Err(o) = require_profiling_engine(app, "trace.start") {
        return StepOutcome::Done(o);
    }
    if let Some(owner) = app.kernel.slots.get(&super::super::BROWSER_RPC).active_trace_host {
        let msg = if owner == host_id {
            "A performance trace is already active in this tab. Use browser.trace.stop({tabID}) to finish it before starting another."
        } else {
            "Another tab owns the active performance trace. Wait for its owner to finish; do not stop or replace another tab's recording."
        };
        return StepOutcome::Done(Outcome::fail(failure_code::OPERATION_FAILED, msg));
    }
    if att.trace_recording.as_ref().is_some_and(|r| r.auto_stopped.is_none()) {
        return StepOutcome::Done(Outcome::fail(
            failure_code::OPERATION_FAILED,
            "A performance trace is already recording. Stop it with browser.trace.stop first.",
        ));
    }
    let duration_ms = cmd
        .action
        .get("durationMs")
        .and_then(Value::as_u64)
        .unwrap_or(TRACE_DEFAULT_DURATION_MS)
        .clamp(1_000, 30_000);
    let resources = match att.source_urls(app, host_id) {
        Ok(urls) => urls,
        Err(e) => return StepOutcome::Done(Outcome::fail(failure_code::OPERATION_FAILED, e)),
    };
    app.workspace_panes.browser.host.set_agent_driven(host_id, true);
    match app.workspace_panes.browser.host.send_typed_cdp(
        host_id,
        "Tracing.start",
        json!({
            "transferMode": "ReportEvents",
            "traceConfig": {
                "recordMode": "recordUntilFull",
                "traceBufferSizeInKb": 8192,
                "includedCategories": [
                    "devtools.timeline",
                    "disabled-by-default-devtools.timeline",
                    "disabled-by-default-devtools.timeline.stack",
                    "v8.execute",
                    "blink.user_timing",
                    "disabled-by-default-v8.cpu_profiler"
                ],
                "excludedCategories": ["*"]
            }
        }),
        None,
    ) {
        Ok(cdp_req) => {
            app.kernel.slots.get_mut(&super::super::BROWSER_RPC).active_trace_host = Some(host_id);
            att.trace_recording = Some(TraceRecording {
                host_id,
                key: format!("trace:{host_id}:{}", new_file_id()),
                started_at: now,
                duration_ms,
                stop_at: None,
                renderer_pid: None,
                events: Vec::new(),
                resources,
                resources_overflow: false,
                complete: false,
                data_loss: false,
                auto_stop_req: None,
                auto_stop_ended: false,
                auto_stop_deadline: None,
                auto_stopped: None,
            });
            StepOutcome::Pending(ExecPhase::Profile(ProfileState {
                host_id,
                stage: ProfileStage::TraceStart { cdp_req },
            }))
        }
        Err(e) => StepOutcome::Done(Outcome::fail(failure_code::OPERATION_FAILED, e)),
    }
}

fn stop_trace(att: &mut SessionAttachment, app: &mut App, cmd: &Command) -> StepOutcome {
    let Some(wire_id) = cmd.tab_id() else {
        return StepOutcome::Done(Outcome::fail(failure_code::OPERATION_FAILED, "Missing tabID."));
    };
    let host_id = match att.resolve_tab(app, wire_id) {
        Ok(id) => id,
        Err(msg) => return StepOutcome::Done(Outcome::fail(failure_code::OPERATION_FAILED, msg)),
    };
    if let Err(o) = check_page_guards(app, host_id, "trace.stop") {
        return StepOutcome::Done(o);
    }
    if att.trace_recording.as_ref().is_none_or(|r| r.host_id != host_id) {
        return StepOutcome::Done(Outcome::fail(
            failure_code::OPERATION_FAILED,
            "No performance trace is currently recording on this tab.",
        ));
    }
    if att.trace_recording.as_ref().is_some_and(|r| r.auto_stopped.is_some()) {
        let Some(rec) = att.trace_recording.as_ref() else {
            return StepOutcome::Done(Outcome::fail(failure_code::OPERATION_FAILED, "Trace unavailable."));
        };
        if rec.resources_overflow {
            att.trace_recording = None;
            return StepOutcome::Done(Outcome::fail(failure_code::OPERATION_FAILED, RESOURCES_OVERFLOW_MSG));
        }
        if !authorized_matches(cmd.target.as_ref(), &rec.key, &rec.resources) {
            return StepOutcome::Done(Outcome::fail(failure_code::DOCUMENT_CHANGED, NAVIGATED_DURING_STOP_MSG));
        }
        let rec = att.trace_recording.take().unwrap();
        return match rec.auto_stopped.unwrap() {
            Ok((file_id, duration_ms, incomplete)) => match wire_file_from_capture(att, host_id, &file_id) {
                Ok(wf) => match att.wire_tab(app, host_id) {
                    Some(tab) => StepOutcome::Done(Outcome::ok(json!({ "tab": tab, "durationMs": duration_ms, "incomplete": incomplete }), vec![wf])),
                    None => StepOutcome::Done(Outcome::fail(failure_code::OPERATION_FAILED, "Tab unavailable.")),
                },
                Err(e) => StepOutcome::Done(Outcome::fail(failure_code::OPERATION_FAILED, e)),
            },
            Err(e) => StepOutcome::Done(Outcome::fail(failure_code::OPERATION_FAILED, e)),
        };
    }
    if let Some(rec) = att.trace_recording.as_mut() {
        rec.stop_at = None;
        rec.auto_stop_deadline = None;
        if let Some(cdp_req) = rec.auto_stop_req.take() {
            return StepOutcome::Pending(ExecPhase::Profile(ProfileState {
                host_id,
                stage: ProfileStage::TraceStop { cdp_req, ended: rec.auto_stop_ended, authorized: cmd.target.clone() },
            }));
        }
    }
    match app.workspace_panes.browser.host.send_typed_cdp(host_id, "Tracing.end", json!({}), None) {
        Ok(cdp_req) => StepOutcome::Pending(ExecPhase::Profile(ProfileState {
            host_id,
            stage: ProfileStage::TraceStop { cdp_req, ended: false, authorized: cmd.target.clone() },
        })),
        Err(e) => StepOutcome::Done(Outcome::fail(failure_code::OPERATION_FAILED, e)),
    }
}

fn start_cpu(att: &mut SessionAttachment, app: &mut App, cmd: &Command, now: Instant) -> StepOutcome {
    let Some(wire_id) = cmd.tab_id() else {
        return StepOutcome::Done(Outcome::fail(failure_code::OPERATION_FAILED, "Missing tabID."));
    };
    let host_id = match att.resolve_tab(app, wire_id) {
        Ok(id) => id,
        Err(msg) => return StepOutcome::Done(Outcome::fail(failure_code::OPERATION_FAILED, msg)),
    };
    if let Err(o) = check_page_guards(app, host_id, "cpu.start") {
        return StepOutcome::Done(o);
    }
    if let Err(o) = require_profiling_engine(app, "cpu.start") {
        return StepOutcome::Done(o);
    }
    if att.cpu_started.get(&host_id).is_some_and(|r| r.auto_stopped.is_none()) {
        return StepOutcome::Done(Outcome::fail(
            failure_code::OPERATION_FAILED,
            "CPU profiling is already active on this tab. Call browser.cpu.stop first.",
        ));
    }
    let resources = match att.source_urls(app, host_id) {
        Ok(urls) => urls,
        Err(e) => return StepOutcome::Done(Outcome::fail(failure_code::OPERATION_FAILED, e)),
    };
    app.workspace_panes.browser.host.set_agent_driven(host_id, true);
    att.cpu_started.insert(host_id, CpuRecording {
        key: format!("cpu:{host_id}:{}", new_file_id()),
        started_at: now,
        stop_at: None,
        resources,
        resources_overflow: false,
        auto_stop_req: None,
        auto_stop_deadline: None,
        auto_stopped: None,
    });
    match app.workspace_panes.browser.host.send_typed_cdp(host_id, "Profiler.enable", json!({}), None) {
        Ok(cdp_req) => StepOutcome::Pending(ExecPhase::Profile(ProfileState {
            host_id,
            stage: ProfileStage::CpuEnable { cdp_req },
        })),
        Err(e) => {
            att.cpu_started.remove(&host_id);
            StepOutcome::Done(Outcome::fail(failure_code::OPERATION_FAILED, e))
        }
    }
}

fn stop_cpu(att: &mut SessionAttachment, app: &mut App, cmd: &Command) -> StepOutcome {
    let Some(wire_id) = cmd.tab_id() else {
        return StepOutcome::Done(Outcome::fail(failure_code::OPERATION_FAILED, "Missing tabID."));
    };
    let host_id = match att.resolve_tab(app, wire_id) {
        Ok(id) => id,
        Err(msg) => return StepOutcome::Done(Outcome::fail(failure_code::OPERATION_FAILED, msg)),
    };
    if let Err(o) = check_page_guards(app, host_id, "cpu.stop") {
        return StepOutcome::Done(o);
    }
    if !att.cpu_started.contains_key(&host_id) {
        return StepOutcome::Done(Outcome::fail(
            failure_code::OPERATION_FAILED,
            "No CPU profile is recording on this tab.",
        ));
    }
    if att.cpu_started.get(&host_id).is_some_and(|r| r.auto_stopped.is_some()) {
        let Some(rec) = att.cpu_started.get(&host_id) else {
            return StepOutcome::Done(Outcome::fail(failure_code::OPERATION_FAILED, "CPU profile unavailable."));
        };
        if rec.resources_overflow {
            att.cpu_started.remove(&host_id);
            return StepOutcome::Done(Outcome::fail(failure_code::OPERATION_FAILED, RESOURCES_OVERFLOW_MSG));
        }
        if !authorized_matches(cmd.target.as_ref(), &rec.key, &rec.resources) {
            return StepOutcome::Done(Outcome::fail(failure_code::DOCUMENT_CHANGED, NAVIGATED_DURING_STOP_MSG));
        }
        let rec = att.cpu_started.remove(&host_id).unwrap();
        return match rec.auto_stopped.unwrap() {
            Ok((file_id, duration_ms)) => match wire_file_from_capture(att, host_id, &file_id) {
                Ok(wf) => match att.wire_tab(app, host_id) {
                    Some(tab) => StepOutcome::Done(Outcome::ok(json!({ "tab": tab, "durationMs": duration_ms }), vec![wf])),
                    None => StepOutcome::Done(Outcome::fail(failure_code::OPERATION_FAILED, "Tab unavailable.")),
                },
                Err(e) => StepOutcome::Done(Outcome::fail(failure_code::OPERATION_FAILED, e)),
            },
            Err(e) => StepOutcome::Done(Outcome::fail(failure_code::OPERATION_FAILED, e)),
        };
    }
    if let Some(rec) = att.cpu_started.get_mut(&host_id) {
        rec.stop_at = None;
        rec.auto_stop_deadline = None;
        if let Some(cdp_req) = rec.auto_stop_req.take() {
            return StepOutcome::Pending(ExecPhase::Profile(ProfileState {
                host_id,
                stage: ProfileStage::CpuStop { cdp_req, authorized: cmd.target.clone() },
            }));
        }
    }
    match app.workspace_panes.browser.host.send_typed_cdp(host_id, "Profiler.stop", json!({}), None) {
        Ok(cdp_req) => StepOutcome::Pending(ExecPhase::Profile(ProfileState {
            host_id,
            stage: ProfileStage::CpuStop { cdp_req, authorized: cmd.target.clone() },
        })),
        Err(e) => StepOutcome::Done(Outcome::fail(failure_code::OPERATION_FAILED, e)),
    }
}

fn start_heap_snapshot(att: &mut SessionAttachment, app: &mut App, cmd: &Command) -> StepOutcome {
    let Some(wire_id) = cmd.tab_id() else {
        return StepOutcome::Done(Outcome::fail(failure_code::OPERATION_FAILED, "Missing tabID."));
    };
    let host_id = match att.resolve_tab(app, wire_id) {
        Ok(id) => id,
        Err(msg) => return StepOutcome::Done(Outcome::fail(failure_code::OPERATION_FAILED, msg)),
    };
    if let Err(o) = check_page_guards(app, host_id, "heap.snapshot") {
        return StepOutcome::Done(o);
    }
    if let Err(o) = require_profiling_engine(app, "heap.snapshot") {
        return StepOutcome::Done(o);
    }
    if att.heap_chunks.contains_key(&host_id) {
        return StepOutcome::Done(Outcome::fail(
            failure_code::OPERATION_FAILED,
            "A heap snapshot is already being captured in this tab.",
        ));
    }
    app.workspace_panes.browser.host.set_agent_driven(host_id, true);
    att.heap_chunks.insert(host_id, (String::new(), false));
    match app.workspace_panes.browser.host.send_typed_cdp(host_id, "HeapProfiler.enable", json!({}), None) {
        Ok(cdp_req) => StepOutcome::Pending(ExecPhase::Profile(ProfileState {
            host_id,
            stage: ProfileStage::HeapEnable { cdp_req },
        })),
        Err(e) => {
            att.heap_chunks.remove(&host_id);
            StepOutcome::Done(Outcome::fail(failure_code::OPERATION_FAILED, e))
        }
    }
}

pub(super) fn step_profile(
    att: &mut SessionAttachment,
    app: &mut App,
    st: &mut ProfileState,
    now: Instant,
) -> Option<Outcome> {
    match &mut st.stage {
        ProfileStage::TraceStart { cdp_req } => {
            let resp = app.workspace_panes.browser.host.take_cdp_response(*cdp_req)?;
            match resp {
                Ok(_) => {
                    if let Some(rec) = att.trace_recording.as_mut()
                        && rec.host_id == st.host_id
                    {
                        rec.started_at = now;
                        rec.stop_at = Some(now + Duration::from_millis(rec.duration_ms));
                    }
                    let tab = att.wire_tab(app, st.host_id)?;
                    Some(Outcome::ok(json!({ "tab": tab, "recording": true }), Vec::new()))
                }
                Err(e) => {
                    att.trace_recording = None;
                    clear_trace_owner(app, st.host_id);
                    Some(Outcome::fail(failure_code::OPERATION_FAILED, cdp_err_msg(&e)))
                }
            }
        }
        ProfileStage::TraceStop { cdp_req, ended, authorized } => {
            if !*ended {
                let resp = app.workspace_panes.browser.host.take_cdp_response(*cdp_req)?;
                if let Err(e) = resp {
                    att.trace_recording = None;
                    clear_trace_owner(app, st.host_id);
                    return Some(Outcome::fail(failure_code::OPERATION_FAILED, cdp_err_msg(&e)));
                }
                *ended = true;
            }
            if !att.trace_recording.as_ref().is_some_and(|r| r.complete) {
                return None;
            }
            let mut rec = att.trace_recording.take()?;
            clear_trace_owner(app, st.host_id);
            let events = std::mem::take(&mut rec.events);
            let saved = finalize_trace(att, app, &mut rec, events, now);
            if rec.resources_overflow {
                return Some(Outcome::fail(failure_code::OPERATION_FAILED, RESOURCES_OVERFLOW_MSG));
            }
            match saved {
                Ok((file_id, duration_ms, incomplete)) => {
                    if !authorized_matches(authorized.as_ref(), &rec.key, &rec.resources) {
                        rec.auto_stopped = Some(Ok((file_id, duration_ms, incomplete)));
                        att.trace_recording = Some(rec);
                        return Some(Outcome::fail(failure_code::DOCUMENT_CHANGED, NAVIGATED_DURING_STOP_MSG));
                    }
                    let wf = match wire_file_from_capture(att, st.host_id, &file_id) {
                        Ok(w) => w,
                        Err(e) => return Some(Outcome::fail(failure_code::OPERATION_FAILED, e)),
                    };
                    let tab = att.wire_tab(app, st.host_id)?;
                    Some(Outcome::ok(json!({ "tab": tab, "durationMs": duration_ms, "incomplete": incomplete }), vec![wf]))
                }
                Err(e) => Some(Outcome::fail(failure_code::OPERATION_FAILED, e)),
            }
        }
        ProfileStage::CpuEnable { cdp_req } => {
            let resp = app.workspace_panes.browser.host.take_cdp_response(*cdp_req)?;
            if let Err(e) = resp {
                att.cpu_started.remove(&st.host_id);
                return Some(Outcome::fail(failure_code::OPERATION_FAILED, cdp_err_msg(&e)));
            }
            match app.workspace_panes.browser.host.send_typed_cdp(st.host_id, "Profiler.start", json!({}), None) {
                Ok(req) => {
                    st.stage = ProfileStage::CpuStart { cdp_req: req };
                    None
                }
                Err(e) => {
                    att.cpu_started.remove(&st.host_id);
                    Some(Outcome::fail(failure_code::OPERATION_FAILED, e))
                }
            }
        }
        ProfileStage::CpuStart { cdp_req } => {
            let resp = app.workspace_panes.browser.host.take_cdp_response(*cdp_req)?;
            match resp {
                Ok(_) => {
                    if let Some(rec) = att.cpu_started.get_mut(&st.host_id) {
                        rec.started_at = now;
                        rec.stop_at = Some(now + Duration::from_millis(CPU_MAX_DURATION_MS));
                    }
                    let tab = att.wire_tab(app, st.host_id)?;
                    Some(Outcome::ok(json!({ "tab": tab, "recording": true }), Vec::new()))
                }
                Err(e) => {
                    att.cpu_started.remove(&st.host_id);
                    Some(Outcome::fail(failure_code::OPERATION_FAILED, cdp_err_msg(&e)))
                }
            }
        }
        ProfileStage::CpuStop { cdp_req, authorized } => {
            let resp = app.workspace_panes.browser.host.take_cdp_response(*cdp_req)?;
            let mut rec = att.cpu_started.remove(&st.host_id)?;
            let _ = app.workspace_panes.browser.host.send_typed_cdp(st.host_id, "Profiler.disable", json!({}), None);
            let saved = finalize_cpu(att, app, st.host_id, &mut rec, resp, now);
            if rec.resources_overflow {
                return Some(Outcome::fail(failure_code::OPERATION_FAILED, RESOURCES_OVERFLOW_MSG));
            }
            match saved {
                Ok((file_id, duration_ms)) => {
                    if !authorized_matches(authorized.as_ref(), &rec.key, &rec.resources) {
                        rec.auto_stopped = Some(Ok((file_id, duration_ms)));
                        att.cpu_started.insert(st.host_id, rec);
                        return Some(Outcome::fail(failure_code::DOCUMENT_CHANGED, NAVIGATED_DURING_STOP_MSG));
                    }
                    let wf = match wire_file_from_capture(att, st.host_id, &file_id) {
                        Ok(w) => w,
                        Err(e) => return Some(Outcome::fail(failure_code::OPERATION_FAILED, e)),
                    };
                    let tab = att.wire_tab(app, st.host_id)?;
                    Some(Outcome::ok(json!({ "tab": tab, "durationMs": duration_ms }), vec![wf]))
                }
                Err(e) => Some(Outcome::fail(failure_code::OPERATION_FAILED, e)),
            }
        }
        ProfileStage::HeapEnable { cdp_req } => {
            let resp = app.workspace_panes.browser.host.take_cdp_response(*cdp_req)?;
            if let Err(e) = resp {
                att.heap_chunks.remove(&st.host_id);
                return Some(Outcome::fail(failure_code::OPERATION_FAILED, cdp_err_msg(&e)));
            }
            match app.workspace_panes.browser.host.send_typed_cdp(
                st.host_id,
                "HeapProfiler.takeHeapSnapshot",
                json!({ "reportProgress": false }),
                None,
            ) {
                Ok(req) => {
                    st.stage = ProfileStage::HeapTake { cdp_req: req };
                    None
                }
                Err(e) => {
                    att.heap_chunks.remove(&st.host_id);
                    Some(Outcome::fail(failure_code::OPERATION_FAILED, e))
                }
            }
        }
        ProfileStage::HeapTake { cdp_req } => {
            let resp = app.workspace_panes.browser.host.take_cdp_response(*cdp_req)?;
            let (raw, overflowed) = att.heap_chunks.remove(&st.host_id).unwrap_or_default();
            let _ = app.workspace_panes.browser.host.send_typed_cdp(st.host_id, "HeapProfiler.disable", json!({}), None);
            if let Err(e) = resp {
                return Some(Outcome::fail(failure_code::OPERATION_FAILED, cdp_err_msg(&e)));
            }
            if overflowed {
                return Some(Outcome::fail(
                    failure_code::OPERATION_FAILED,
                    "Heap snapshot exceeded the 64 MiB uncompressed size limit.",
                ));
            }
            let gz = match gzip_bytes(raw.as_bytes()) {
                Ok(b) => b,
                Err(e) => return Some(Outcome::fail(failure_code::OPERATION_FAILED, e)),
            };
            let file_id = new_file_id();
            let wf = match WireFile::from_bytes(file_id.clone(), "heap.heapsnapshot.gz".into(), "application/gzip".into(), &gz) {
                Ok(w) => w,
                Err(e) => return Some(Outcome::fail(failure_code::OPERATION_FAILED, e)),
            };
            let resources = match att.source_urls(app, st.host_id) {
                Ok(urls) => urls,
                Err(e) => return Some(Outcome::fail(failure_code::OPERATION_FAILED, e)),
            };
            att.record_capture(st.host_id, CapturedFile { id: file_id, name: "heap.heapsnapshot.gz".into(), mime: "application/gzip".into(), bytes: gz, resources });
            let tab = att.wire_tab(app, st.host_id)?;
            Some(Outcome::ok(json!({ "tab": tab }), vec![wf]))
        }
    }
}
