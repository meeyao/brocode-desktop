//! Desktop Tier 2 CDP Freeze, Tier 3 Hibernation triggers/blockers, and seamless user/extension wake.

use std::time::Duration;

use browser_pane::{ContextKind, DownloadState, PermissionKind, SiteSetting, TabResidency};
use rune::time::Instant;

use crate::App;

/// How long an eligible hidden browser tab stays unfrozen before Tier 2 CDP Freeze triggers.
pub const FREEZE_AFTER: Duration = Duration::from_secs(120);

/// How long an eligible hidden/frozen browser tab stays inactive before Tier 3 Hibernation triggers.
pub const HIBERNATE_AFTER: Duration = Duration::from_secs(15 * 60);

/// Pure eligibility predicate for Tier 2 CDP Freeze on `tab_id`.
pub fn can_freeze_tab(app: &App, tab_id: u64) -> bool {
    let browser = &app.workspace_panes.browser;
    let host = &browser.host;
    if !host.has_live_sidecar() {
        return false;
    }
    let Some(t) = host.tab(tab_id) else {
        return false;
    };
    if host.visible_tabs().contains(&tab_id) || host.is_tab_unthrottled(tab_id) {
        return false;
    }
    if !t.has_engine_tab() || !t.engine_confirmed() || t.loading || t.audible || t.inspecting {
        return false;
    }
    if t.prompts.waiting_on_user() {
        return false;
    }
    if t.files.iter().any(|f| f.state == DownloadState::Pending) {
        return false;
    }
    if browser.save_dialog_open || host.has_download_save_requests() {
        return false;
    }
    if browser.devtools.values().any(|p| p.tab_id == tab_id) {
        return false;
    }
    if browser.overlay.active_for(tab_id) {
        return false;
    }
    if browser.active_app_reviews.borrow().contains_key(&tab_id) {
        return false;
    }
    if host.has_extension_popup_for(tab_id) {
        return false;
    }
    if host.tab_has_notification_grant(tab_id) {
        return false;
    }
    let has_media_grant = t.prompts.visit_permissions.iter().any(|((_, kind), setting)| {
        *setting == SiteSetting::Allow
            && matches!(kind, PermissionKind::Camera | PermissionKind::Microphone)
    });
    !has_media_grant
}

/// Pure eligibility predicate for Tier 3 Hibernation (`HostMsg::HibernateTab`) on `tab_id`.
pub fn can_hibernate_tab(app: &App, tab_id: u64, for_session_close: bool) -> bool {
    let browser = &app.workspace_panes.browser;
    let host = &browser.host;
    if !host.has_live_sidecar() || !host.engine_tab_hibernation() {
        return false;
    }
    let Some(t) = host.tab(tab_id) else {
        return false;
    };
    if !t.has_engine_tab()
        || !t.engine_confirmed()
        || matches!(t.residency(), TabResidency::Hibernated { .. } | TabResidency::Waking)
    {
        return false;
    }
    // Hard blockers (apply even on session close / archive).
    if t.loading || t.prompts.waiting_on_user() || t.refusal.is_some() {
        return false;
    }
    if t.files.iter().any(|f| f.state == DownloadState::Pending)
        || browser.save_dialog_open
        || host.has_download_save_requests()
    {
        return false;
    }
    if browser.devtools.values().any(|p| p.tab_id == tab_id) {
        return false;
    }
    if host.has_extension_popup_for(tab_id) {
        return false;
    }
    if super::rpc::is_tab_or_session_busy(app, tab_id) {
        return false;
    }
    if host.is_tab_unthrottled(tab_id) {
        return false;
    }
    if t.url.is_empty() || t.url.eq_ignore_ascii_case("about:blank") || browser.openers.is_unnamed(tab_id) {
        return false;
    }
    if for_session_close {
        // Explicit session close/archive intentionally hibernates `ContextKind::Private` and
        // `RemoteServer` tabs too (matching official `pane-machine.ts` `release-pages` disposal).
        return true;
    }
    // Additional blockers for idle background hibernation while the session stays open.
    if host.visible_tabs().contains(&tab_id) || t.audible || t.inspecting {
        return false;
    }
    if host.tab_has_notification_grant(tab_id) {
        return false;
    }
    let has_media_grant = t.prompts.visit_permissions.iter().any(|((_, kind), setting)| {
        *setting == SiteSetting::Allow
            && matches!(kind, PermissionKind::Camera | PermissionKind::Microphone)
    });
    if has_media_grant {
        return false;
    }
    if browser.overlay.active_for(tab_id)
        || browser.active_app_reviews.borrow().contains_key(&tab_id)
    {
        return false;
    }
    if matches!(t.context, ContextKind::Private) {
        return false;
    }
    // Keep the last live tab of an open `RemoteServer` at Tier 2 Freeze so remote cookies survive.
    if let ContextKind::RemoteServer { server_id, .. } = &t.context {
        let has_other_live = host.tabs().iter().any(|other| {
            other.id != tab_id
                && other.has_engine_tab()
                && !other.residency().is_hibernated()
                && matches!(&other.context, ContextKind::RemoteServer { server_id: oid, .. } if oid == server_id)
        });
        if !has_other_live {
            return false;
        }
    }
    true
}

impl App {
    /// Canonical Tier 3 hibernation: ensures `tab_id` is hidden, tears down its CEF browser via
    /// `BrowserHost::hibernate_tab`, revokes any in-flight preview fetch binding, and notifies
    /// `browser::rpc::on_tab_hibernated` (clearing frames/refs and bumping generation ON HIBERNATE).
    pub fn hibernate_browser_tab(&mut self, tab_id: u64) -> bool {
        let can_attempt = {
            let host = &self.workspace_panes.browser.host;
            host.has_live_sidecar()
                && host.engine_tab_hibernation()
                && !host.is_tab_unthrottled(tab_id)
                && host.tab(tab_id).is_some_and(|t| {
                    t.has_engine_tab()
                        && !t.loading
                        && !matches!(t.residency(), TabResidency::Hibernated { .. })
                })
        };
        if !can_attempt {
            return false;
        }
        if self.workspace_panes.browser.host.visible_tabs().contains(&tab_id) {
            let next: Vec<u64> = self
                .workspace_panes
                .browser
                .host
                .visible_tabs()
                .iter()
                .copied()
                .filter(|&id| id != tab_id)
                .collect();
            self.workspace_panes.browser.host.set_visible_tabs(&next);
        }
        if !self.workspace_panes.browser.host.hibernate_tab(tab_id) {
            return false;
        }
        self.revoke_preview_binding(tab_id);
        super::rpc::on_tab_hibernated(self, tab_id);
        true
    }

    fn is_browser_owner_open_or_routed(&self, owner: &str) -> bool {
        let in_open_tab = self.tabs.iter().any(|t| {
            !t.is_draft() && (t.id == owner || self.ancestors(owner).any(|a| a == t.id))
        });
        let is_routed = matches!(&*self.route, crate::Route::Session { id, .. }
            if id == owner
                || self.ancestors(id).any(|a| a == owner)
                || self.ancestors(owner).any(|a| a == id.as_str())
        );
        in_open_tab || is_routed
    }

    /// Hibernates eligible live browser tabs belonging to session `sid` and its subagent family
    /// (matching official `pane-machine.ts` `session.presence { listed: false }` -> `release-pages`),
    /// skipping any child subagent whose own tab or family route is still open.
    /// Preserves `BrowserTab` metadata and strip entries so `reopen_tab` and Undo Archive restore them.
    pub fn hibernate_browser_tabs_for_session(&mut self, sid: &str) {
        let candidates: Vec<u64> = self
            .workspace_panes
            .browser
            .session_tabs
            .iter()
            .filter(|(owner, _)| {
                (*owner == sid || self.ancestors(owner).any(|a| a == sid))
                    && !self.is_browser_owner_open_or_routed(owner)
            })
            .flat_map(|(_, ids)| ids.iter().copied())
            .collect();
        for tab_id in candidates {
            if can_hibernate_tab(self, tab_id, true) {
                self.hibernate_browser_tab(tab_id);
            }
        }
    }

    /// Called from `App::rest`: hibernates any remaining live browser tabs whose owning session
    /// is neither open in `self.tabs` nor currently routed (catches tabs that were busy/loading
    /// when their session tab closed and later became idle).
    pub fn hibernate_orphaned_browser_tabs(&mut self) {
        let host = &self.workspace_panes.browser.host;
        if !host.has_live_sidecar() || !host.engine_tab_hibernation() {
            return;
        }
        let candidates: Vec<u64> = self
            .workspace_panes
            .browser
            .session_tabs
            .iter()
            .filter(|(owner, _)| !self.is_browser_owner_open_or_routed(owner))
            .flat_map(|(_, ids)| ids.iter().copied())
            .collect();
        for tab_id in candidates {
            if can_hibernate_tab(self, tab_id, true) {
                self.hibernate_browser_tab(tab_id);
            }
        }
    }

    /// Returns `false` when `tab_id` is a remote/artifact preview whose owning server is offline
    /// or signed out so callers never spawn a CEF browser into a sticky 403.
    pub(super) fn preview_server_ready_for_wake(&self, tab_id: u64) -> bool {
        let needs_server = self
            .workspace_panes
            .browser
            .host
            .tab(tab_id)
            .and_then(|t| t.preview_spec.as_ref())
            .is_some_and(|spec| {
                matches!(
                    spec,
                    browser_pane::PreviewSpec::Remote { .. }
                        | browser_pane::PreviewSpec::Artifact { .. }
                )
            });
        if !needs_server {
            return true;
        }
        let sid = self.browser_tab_session(tab_id);
        self.preview_server_for(sid).is_some()
    }

    /// Re-establishes `PreviewTabBinding` if a live preview tab lost its binding (e.g. after
    /// server sign-out/reconnect) before issuing a user reload.
    pub fn ensure_preview_binding(&mut self, tab_id: u64) {
        let needs_bind = self
            .workspace_panes
            .browser
            .host
            .tab(tab_id)
            .is_some_and(|t| t.preview_spec.is_some())
            && !self.workspace_panes.browser.preview_bindings.contains_key(&tab_id);
        if needs_bind {
            let sid = self.browser_tab_session(tab_id).map(str::to_owned);
            self.bind_preview_tab(tab_id, sid.as_deref());
        }
    }

    /// Wakes a hibernated browser tab at its preserved URL and context, re-establishing any
    /// `PreviewTabBinding` for isolated preview/artifact tabs first. Fails closed without spawning
    /// a CEF browser when a remote/artifact preview's owning server is signed out or offline.
    pub fn wake_browser_tab(&mut self, tab_id: u64, focused: bool) -> bool {
        let is_preview = self
            .workspace_panes
            .browser
            .host
            .tab(tab_id)
            .is_some_and(|t| t.residency().is_hibernated() && t.preview_spec.is_some());
        if is_preview {
            if !self.preview_server_ready_for_wake(tab_id) {
                return false;
            }
            let sid = self.browser_tab_session(tab_id).map(str::to_owned);
            self.bind_preview_tab(tab_id, sid.as_deref());
        }
        let woke = self.workspace_panes.browser.host.wake_tab(tab_id, focused);
        if !woke && is_preview {
            self.revoke_preview_binding(tab_id);
        }
        woke
    }

    /// Wakes a hibernated non-preview browser tab directly to `url` in `context`.
    pub fn wake_browser_tab_to_url(
        &mut self,
        tab_id: u64,
        url: &str,
        context: ContextKind,
        focused: bool,
    ) -> bool {
        self.workspace_panes
            .browser
            .host
            .wake_tab_to_url(tab_id, url, context, focused)
    }

    /// Freezes eligible `Hidden` tabs reaching [`FREEZE_AFTER`] and hibernates eligible
    /// `Hidden`/`Frozen` tabs reaching [`HIBERNATE_AFTER`]. Read-only fast exit when disconnected
    /// or when no tab is due so `Tracked<BrowserPane>` is never dirtied needlessly.
    pub fn tick_browser_residency(&mut self) {
        let host = &self.workspace_panes.browser.host;
        if !host.has_live_sidecar() || !host.engine_tab_hibernation() {
            return;
        }
        let now = Instant::now();
        let mut to_hibernate = Vec::new();
        let mut to_freeze = Vec::new();
        for t in host.tabs() {
            if let Some(since) = t.residency().inactive_since()
                && now.saturating_duration_since(since) >= HIBERNATE_AFTER
                && can_hibernate_tab(self, t.id, false)
            {
                to_hibernate.push(t.id);
                continue;
            }
            if let Some(since) = t.residency().hidden_since()
                && now.saturating_duration_since(since) >= FREEZE_AFTER
                && can_freeze_tab(self, t.id)
            {
                to_freeze.push(t.id);
            }
        }
        if to_hibernate.is_empty() && to_freeze.is_empty() {
            return;
        }
        for tab_id in to_hibernate {
            // Re-check right before hibernating so the last live tab of an open `RemoteServer`
            // context is preserved even if multiple tabs cross `HIBERNATE_AFTER` on the same tick.
            if can_hibernate_tab(self, tab_id, false) {
                self.hibernate_browser_tab(tab_id);
            }
        }
        for tab_id in to_freeze {
            self.workspace_panes.browser.host.freeze_tab(tab_id, true);
        }
    }

    /// Earliest deadline at which an eligible hidden/frozen browser tab reaches [`FREEZE_AFTER`]
    /// or [`HIBERNATE_AFTER`]. Strictly gated on `has_live_sidecar() && engine_tab_hibernation()`
    /// and eligibility predicates so it never spins.
    pub fn browser_residency_wake(&self) -> Option<Instant> {
        let host = &self.workspace_panes.browser.host;
        if !host.has_live_sidecar() || !host.engine_tab_hibernation() {
            return None;
        }
        let mut earliest: Option<Instant> = None;
        for t in host.tabs() {
            if let Some(since) = t.residency().hidden_since()
                && can_freeze_tab(self, t.id)
            {
                let at = since + FREEZE_AFTER;
                earliest = Some(earliest.map_or(at, |cur| cur.min(at)));
            }
            if let Some(since) = t.residency().inactive_since()
                && can_hibernate_tab(self, t.id, false)
            {
                let at = since + HIBERNATE_AFTER;
                earliest = Some(earliest.map_or(at, |cur| cur.min(at)));
            }
        }
        earliest
    }

    /// Processes queued `EngineMsg::ExtensionWakeTab` requests emitted when an extension targets
    /// a discarded (hibernated) tab via `chrome.tabs.{update, reload, highlight}`.
    pub fn drain_extension_wake_tabs(&mut self) -> bool {
        if !self.workspace_panes.browser.host.has_pending_ext_wakes() {
            return false;
        }
        let wakes: Vec<_> = self
            .workspace_panes
            .browser
            .host
            .drain_extension_wake_tabs()
            .collect();
        let mut changed = false;
        for req in wakes {
            let state = self.workspace_panes.browser.host.tab(req.tab_id).and_then(|t| {
                // Re-verify desktop-side that the tab is still extension-visible before acting.
                if t.prompts.agent_driven || t.context.is_isolated() || t.preview.is_some() {
                    return None;
                }
                Some((t.residency().is_hibernated(), t.context.clone()))
            });
            let Some((hibernated, ctx)) = state else {
                continue;
            };
            if hibernated {
                if let Some(url) = req.url {
                    self.wake_browser_tab_to_url(req.tab_id, &url, ctx, req.focus);
                } else {
                    self.wake_browser_tab(req.tab_id, req.focus);
                }
            } else {
                if let Some(url) = req.url {
                    self.workspace_panes
                        .browser
                        .host
                        .navigate_with_policy(req.tab_id, &url, false);
                } else if req.reload {
                    self.workspace_panes.browser.host.reload(req.tab_id);
                }
                if req.focus {
                    self.workspace_panes.browser.host.focus_tab(req.tab_id);
                }
            }
            changed = true;
        }
        changed
    }
}
