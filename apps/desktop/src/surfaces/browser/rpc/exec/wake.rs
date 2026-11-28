//! Transparent agent `experimental.browser` v4 auto-wake state machine for hibernated tabs,
//! plus RPC busy/hibernation hooks for desktop Tier 3 residency.

use std::time::Duration;

use opencode_kit::browser_rpc::{Command, Outcome, failure_code};
use rune::time::Instant;
use serde_json::Value;

use super::super::hold;
use super::super::session::SessionAttachment;
use super::{ExecPhase, StepOutcome};
use crate::App;

/// Maximum time to wait for a hibernated tab to confirm and settle its initial load.
const WAKE_TIMEOUT: Duration = Duration::from_secs(15);
/// Brief post-confirm grace before accepting `!tab.loading` so we do not complete before
/// `loading` flips true.
const WAKE_COMMIT_GRACE: Duration = Duration::from_millis(1_200);
/// Maximum post-load wait for the main frame's default execution context (so empty/error pages
/// never hang forever).
const WAKE_EVAL_CONTEXT_GRACE: Duration = Duration::from_secs(2);

/// How an `experimental.browser` command interacts with a hibernated or waking tab.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum WakePolicy {
    /// Reads desktop-retained state or closes/stops without waking the renderer.
    NoWake,
    /// Requires element refs that were cleared on hibernate; let normal lookup fail `STALE_REF`.
    ImmediateStaleRef,
    /// Navigates or reloads directly to its target URL without double-loading the old URL.
    DirectTarget,
    /// Must wake the tab and settle its document before inspecting or executing.
    WakeBeforeDispatch,
}

/// Classifies `cmd` into its explicit [`WakePolicy`].
pub(super) fn classify_command(cmd: &Command) -> WakePolicy {
    let Some(ty) = cmd.action_type() else {
        return WakePolicy::NoWake;
    };
    let has_ref = cmd.action.get("ref").and_then(Value::as_str).is_some();
    match ty {
        "tabs.list"
        | "tabs.open"
        | "tabs.close"
        | "stop"
        | "console"
        | "network.list"
        | "network.get"
        | "files.list"
        | "files.get"
        | "dialog"
        | "trace.stop"
        | "trace.analyze"
        | "cpu.stop"
        | "cpu.analyze"
        | "heap.summary"
        | "heap.query"
        | "heap.object"
        | "heap.compare"
        | "lighthouse"
        | "back"
        | "forward" => WakePolicy::NoWake,
        "click"
        | "hover"
        | "drag"
        | "fill"
        | "fill_form"
        | "select"
        | "check"
        | "files.upload"
        | "files.drop" => WakePolicy::ImmediateStaleRef,
        "snapshot" | "find" | "evaluate" | "screenshot" | "scroll" | "press" if has_ref => {
            WakePolicy::ImmediateStaleRef
        }
        "navigate" | "navigate.path" | "preview" | "reload" => WakePolicy::DirectTarget,
        "tabs.focus"
        | "frames"
        | "snapshot"
        | "find"
        | "evaluate"
        | "press"
        | "scroll"
        | "wait"
        | "screenshot"
        | "trace.start"
        | "cpu.start"
        | "heap.snapshot" => WakePolicy::WakeBeforeDispatch,
        _ if cmd.tab_id().is_some() => WakePolicy::WakeBeforeDispatch,
        _ => WakePolicy::NoWake,
    }
}

/// Whether `cmd` requires the main frame's default JS execution context after load settles.
pub(super) fn needs_eval_context(cmd: &Command) -> bool {
    match cmd.action_type() {
        Some("evaluate") => true,
        Some("wait") => matches!(
            cmd.action.get("condition").and_then(Value::as_str),
            Some("text" | "textGone")
        ),
        _ => false,
    }
}

/// Explicit stage of a pending tab wake.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WakeStage {
    /// Awaiting `has_engine_tab && engine_confirmed` from the sidecar.
    Confirm,
    /// Confirmed by engine at `confirmed_at`; awaiting initial load completion.
    Load { saw_loading: bool, confirmed_at: Instant },
    /// Load settled; awaiting main-frame `Runtime.executionContextCreated` (bounded by grace).
    EvalContext { settled_at: Instant },
}

enum WakeEvent {
    Tick {
        confirmed: bool,
        loading: bool,
        has_eval_context: bool,
        needs_eval_context: bool,
        deadline: Instant,
        now: Instant,
    },
}

enum StageStep {
    Wait,
    Ready,
    TimedOut,
}

fn step_stage(stage: &mut WakeStage, ev: WakeEvent) -> StageStep {
    let WakeEvent::Tick {
        confirmed,
        loading,
        has_eval_context,
        needs_eval_context,
        deadline,
        now,
    } = ev;
    if now >= deadline {
        return StageStep::TimedOut;
    }
    if !confirmed {
        return StageStep::Wait;
    }
    if *stage == WakeStage::Confirm {
        *stage = WakeStage::Load { saw_loading: loading, confirmed_at: now };
    }
    match stage {
        WakeStage::Confirm => StageStep::Wait,
        WakeStage::Load { saw_loading, confirmed_at } => {
            if loading {
                *saw_loading = true;
                return StageStep::Wait;
            }
            if !*saw_loading && now.saturating_duration_since(*confirmed_at) < WAKE_COMMIT_GRACE {
                return StageStep::Wait;
            }
            if needs_eval_context && !has_eval_context {
                *stage = WakeStage::EvalContext { settled_at: now };
                return StageStep::Wait;
            }
            StageStep::Ready
        }
        WakeStage::EvalContext { settled_at } => {
            if has_eval_context
                || now.saturating_duration_since(*settled_at) >= WAKE_EVAL_CONTEXT_GRACE
            {
                StageStep::Ready
            } else {
                StageStep::Wait
            }
        }
    }
}

/// Non-blocking state while waking a hibernated tab before inspecting or dispatching `cmd`.
pub(super) struct WakeState {
    pub host_id: u64,
    pub cmd: Command,
    pub inspect: bool,
    pub needs_eval_context: bool,
    pub deadline: Instant,
    stage: WakeStage,
}

pub(super) enum WakePoll {
    Wait,
    Ready,
    Failed(Outcome),
}

impl WakeState {
    fn new(host_id: u64, cmd: Command, now: Instant) -> Self {
        let inspect = cmd.is_inspect();
        let needs_eval = needs_eval_context(&cmd);
        Self {
            host_id,
            cmd,
            inspect,
            needs_eval_context: needs_eval,
            deadline: now + WAKE_TIMEOUT,
            stage: WakeStage::Confirm,
        }
    }

    pub fn next_wake(&self) -> Instant {
        match self.stage {
            // Engine snapshots wake the poll immediately on confirmation; never spin during cold start.
            WakeStage::Confirm | WakeStage::Load { saw_loading: true, .. } => self.deadline,
            WakeStage::Load { saw_loading: false, confirmed_at } => {
                (confirmed_at + WAKE_COMMIT_GRACE).min(self.deadline)
            }
            WakeStage::EvalContext { settled_at } => {
                (settled_at + WAKE_EVAL_CONTEXT_GRACE).min(self.deadline)
            }
        }
    }

    pub fn cancel_cleanup(&mut self, att: &SessionAttachment, app: &mut App, other_waking: bool) {
        let visible = app
            .workspace_panes
            .browser
            .host
            .visible_tabs()
            .contains(&self.host_id);
        let still_waking = other_waking
            || att
                .active_commands
                .iter()
                .any(|c| c.is_waking_tab(self.host_id));
        if !visible
            && !still_waking
            && app.workspace_panes.browser.host.tab_open_in_engine(self.host_id)
        {
            app.workspace_panes.browser.host.stop(self.host_id);
        }
    }
}

/// Begins waking a hibernated (or already-waking) tab for a `WakeBeforeDispatch` command.
pub(super) fn begin_wake(
    att: &mut SessionAttachment,
    app: &mut App,
    host_id: u64,
    cmd: Command,
    now: Instant,
) -> StepOutcome {
    hold::acquire_now(att, app, host_id);
    let is_hibernated = app
        .workspace_panes
        .browser
        .host
        .tab(host_id)
        .is_some_and(|t| t.residency().is_hibernated());
    if is_hibernated {
        let focus = cmd.action_type() == Some("tabs.focus") && !cmd.is_inspect();
        if !app.wake_browser_tab(host_id, focus) {
            let reason = app
                .workspace_panes
                .browser
                .host
                .tab(host_id)
                .and_then(|t| t.refusal.clone())
                .unwrap_or_else(|| "browser.wake.failed".to_owned());
            return StepOutcome::Done(Outcome::fail(
                failure_code::NAVIGATION_FAILED,
                format!("Could not wake hibernated browser tab ({reason})."),
            ));
        }
    }
    StepOutcome::Pending(ExecPhase::Wake(WakeState::new(host_id, cmd, now)))
}

/// Advances a pending [`WakeState`] by one poll tick.
pub(super) fn step_wake(
    att: &mut SessionAttachment,
    app: &mut App,
    st: &mut WakeState,
    now: Instant,
) -> WakePoll {
    match app.workspace_panes.browser.host.status() {
        browser_pane::EngineStatus::Ready => {}
        browser_pane::EngineStatus::Failed(why) => {
            if matches!(why.fault, browser_pane::Fault::NotPublished) {
                return WakePoll::Failed(Outcome::fail(
                    failure_code::UNSUPPORTED,
                    "Embedded browser engine is unavailable in this build.",
                ));
            }
            return WakePoll::Failed(Outcome::fail(
                failure_code::NAVIGATION_FAILED,
                format!("Browser engine failed to start: {}", why.fault),
            ));
        }
        browser_pane::EngineStatus::Unsupported => {
            return WakePoll::Failed(Outcome::fail(
                failure_code::UNSUPPORTED,
                "Embedded browser engine is unavailable in this build.",
            ));
        }
        _ => {
            if now >= st.deadline {
                return WakePoll::Failed(Outcome::fail(
                    failure_code::NAVIGATION_FAILED,
                    "Timed out waiting for the browser engine to become ready.",
                ));
            }
            return WakePoll::Wait;
        }
    }

    let Some((residency, confirmed, loading, refusal)) = app
        .workspace_panes
        .browser
        .host
        .tab(st.host_id)
        .map(|t| (t.residency(), t.has_engine_tab() && t.engine_confirmed(), t.loading, t.refusal.clone()))
    else {
        return WakePoll::Failed(Outcome::fail(
            failure_code::OPERATION_FAILED,
            "Browser tab is unavailable.",
        ));
    };

    if let Some(code) = refusal {
        return WakePoll::Failed(Outcome::fail(
            failure_code::NAVIGATION_FAILED,
            format!("Failed to wake hibernated browser tab ({code})."),
        ));
    }

    if residency.is_hibernated() {
        return WakePoll::Failed(Outcome::fail(
            failure_code::NAVIGATION_FAILED,
            "Failed to wake hibernated browser tab.",
        ));
    }

    if confirmed {
        att.ensure_tab_cdp(app, st.host_id);
    }

    let has_eval = att.frames.get(&st.host_id).is_some_and(|f| {
        f.main_frame_id
            .as_deref()
            .is_some_and(|fid| f.main_contexts.contains_key(fid))
    });

    match step_stage(
        &mut st.stage,
        WakeEvent::Tick {
            confirmed,
            loading,
            has_eval_context: has_eval,
            needs_eval_context: st.needs_eval_context,
            deadline: st.deadline,
            now,
        },
    ) {
        StageStep::Wait => WakePoll::Wait,
        StageStep::Ready => WakePoll::Ready,
        StageStep::TimedOut => WakePoll::Failed(Outcome::fail(
            failure_code::NAVIGATION_FAILED,
            format!("Timed out after {}s waking hibernated browser tab.", WAKE_TIMEOUT.as_secs()),
        )),
    }
}

/// Returns `true` when `host_id` is hibernated or waking so `WaitLoad` treats the navigation
/// as a fresh engine tab load rather than settling on a prior wake's commit.
pub(super) fn is_fresh_residency(app: &App, host_id: u64) -> bool {
    app.workspace_panes
        .browser
        .host
        .tab(host_id)
        .is_some_and(|t| t.residency().is_hibernated() || t.residency().is_waking())
}

/// Reloads `host_id`, waking a hibernated tab at its saved URL (and avoiding a duplicate reload
/// if the tab is already waking).
pub(super) fn reload_or_wake(app: &mut App, host_id: u64, ignore_cache: bool) -> Result<(), Outcome> {
    let residency = app.workspace_panes.browser.host.tab(host_id).map(|t| t.residency());
    if matches!(residency, Some(browser_pane::TabResidency::Hibernated { .. })) {
        if !app.wake_browser_tab(host_id, false) {
            let reason = app
                .workspace_panes
                .browser
                .host
                .tab(host_id)
                .and_then(|t| t.refusal.clone())
                .unwrap_or_else(|| "browser.wake.failed".to_owned());
            return Err(Outcome::fail(
                failure_code::NAVIGATION_FAILED,
                format!("Could not wake hibernated browser tab ({reason})."),
            ));
        }
    } else if !matches!(residency, Some(browser_pane::TabResidency::Waking)) {
        if ignore_cache {
            app.workspace_panes.browser.host.reload_ignore_cache(host_id);
        } else {
            app.workspace_panes.browser.host.reload(host_id);
        }
    }
    Ok(())
}

/// Returns `true` if `tab_id` or its owning session currently has active agent RPC work,
/// live trace/CPU/heap profiling, or a busy agent turn.
pub fn is_tab_or_session_busy(app: &App, tab_id: u64) -> bool {
    if let Some(sid) = app.browser_tab_session(tab_id)
        && (app.session_busy(sid) || app.subtree_busy(sid))
    {
        return true;
    }
    let st = app.kernel.slots.get(&super::super::BROWSER_RPC);
    if st.active_trace_host == Some(tab_id) {
        return true;
    }
    st.sessions.values().any(|att| {
        att.active_commands.iter().any(|c| c.target_host_tab() == tab_id)
            || att
                .trace_recording
                .as_ref()
                .is_some_and(|r| r.host_id == tab_id && r.auto_stopped.is_none())
            || att
                .cpu_started
                .get(&tab_id)
                .is_some_and(|r| r.auto_stopped.is_none())
            || att.heap_chunks.contains_key(&tab_id)
            || (att.host_to_wire.contains_key(&tab_id)
                && (app.session_busy(&att.session_id) || app.subtree_busy(&att.session_id)))
    })
}

/// Clears renderer-scoped RPC state (`frames`, `refs`, `cdp_bootstrapped`, `bootstrap_frame_reqs`)
/// and bumps `generation` ON HIBERNATE so pre-hibernation authorizations are invalidated
/// immediately, while wire tab IDs, diagnostics, captures, and downloads stay intact. Subsequent
/// `inspect: true` calls wake the tab before computing `target`/`generation`, and direct
/// authorized commands against a hibernated/waking tab fail `DOCUMENT_CHANGED` for re-inspection.
pub fn on_tab_hibernated(app: &mut App, host_id: u64) {
    let st = app.kernel.slots.get_mut(&super::super::BROWSER_RPC);
    for att in st.sessions.values_mut() {
        if att.host_to_wire.contains_key(&host_id) {
            att.frames.remove(&host_id);
            att.refs.remove(&host_id);
            att.cdp_bootstrapped.remove(&host_id);
            att.bootstrap_frame_reqs.retain(|(h, _, _)| *h != host_id);
            att.bump_generation(host_id);
        }
    }
}
