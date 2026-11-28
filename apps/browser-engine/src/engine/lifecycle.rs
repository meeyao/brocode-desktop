//! Explicit per-tab visibility, frame-throttling, Tier 2 CDP Freeze, and Tier 3 Hibernate state
//! machine for `EngineManager`.
//!
//! Centralizes `was_hidden`, windowless FPS throttling (1 FPS hidden vs display-matched
//! `pane_fps()` awake), late-frame blocking (`ManagedTab::awake`), keyboard focus restore,
//! Windows D3D11 / Linux dmabuf GPU frame ring release, Chromium Page Lifecycle freeze/unfreeze
//! (`Page.setWebLifecycleState`), and Tier 3 renderer
//! teardown (`HibernateTab`) while preserving completed downloads, staged uploads, and extension
//! discarded tab state.

use std::sync::atomic::Ordering;

use cef::{ImplBrowserHost, PaintElementType};
use serde_json::json;

use super::{EngineManager, ManagedTab};
use crate::wire::EngineMsg;

/// Whether the connected host drives visibility explicitly (`SetVisibleTabs` / `SetTabUnthrottled`)
/// or uses legacy single-active `FocusTab` semantics.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum LifecycleMode {
    #[default]
    Legacy,
    Explicit,
}

/// Explicit per-tab rendering mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TabRenderMode {
    Hidden { frozen: bool },
    AwakeBackground,
    Visible,
}

impl Default for TabRenderMode {
    fn default() -> Self {
        Self::Hidden { frozen: false }
    }
}

impl TabRenderMode {
    pub(crate) fn is_awake(self) -> bool {
        !matches!(self, Self::Hidden { .. })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RenderModeEvent {
    Sync { visible: bool, held: bool },
    SetFrozen(bool),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RenderModeEffect {
    None,
    Wake { visible: bool, unfreeze: bool },
    BecameVisible,
    BecameBackground,
    Sleep,
    Freeze,
    UnfreezeHidden,
}

fn step_render_mode(state: &mut TabRenderMode, event: RenderModeEvent) -> RenderModeEffect {
    match (*state, event) {
        (TabRenderMode::Hidden { frozen }, RenderModeEvent::Sync { visible: true, .. }) => {
            *state = TabRenderMode::Visible;
            RenderModeEffect::Wake { visible: true, unfreeze: frozen }
        }
        (TabRenderMode::Hidden { frozen }, RenderModeEvent::Sync { visible: false, held: true }) => {
            *state = TabRenderMode::AwakeBackground;
            RenderModeEffect::Wake { visible: false, unfreeze: frozen }
        }
        (TabRenderMode::Hidden { .. }, RenderModeEvent::Sync { visible: false, held: false }) => {
            RenderModeEffect::None
        }
        (TabRenderMode::AwakeBackground, RenderModeEvent::Sync { visible: true, .. }) => {
            *state = TabRenderMode::Visible;
            RenderModeEffect::BecameVisible
        }
        (TabRenderMode::Visible, RenderModeEvent::Sync { visible: false, held: true }) => {
            *state = TabRenderMode::AwakeBackground;
            RenderModeEffect::BecameBackground
        }
        (TabRenderMode::Visible | TabRenderMode::AwakeBackground, RenderModeEvent::Sync { visible: false, held: false }) => {
            *state = TabRenderMode::Hidden { frozen: false };
            RenderModeEffect::Sleep
        }
        (TabRenderMode::Hidden { frozen: false }, RenderModeEvent::SetFrozen(true)) => {
            *state = TabRenderMode::Hidden { frozen: true };
            RenderModeEffect::Freeze
        }
        (TabRenderMode::Hidden { frozen: true }, RenderModeEvent::SetFrozen(false)) => {
            *state = TabRenderMode::Hidden { frozen: false };
            RenderModeEffect::UnfreezeHidden
        }
        _ => RenderModeEffect::None,
    }
}

/// Applies `effect` to `t` and returns `Some(frozen)` when `ExtensionHost` should be notified
/// of a frozen-state transition.
fn apply_render_effect(
    next_cdp_id: &std::sync::atomic::AtomicI32,
    id: u64,
    t: &mut ManagedTab,
    visible: bool,
    focused: bool,
    effect: RenderModeEffect,
) -> Option<bool> {
    if effect == RenderModeEffect::None {
        return None;
    }
    #[cfg(target_os = "linux")]
    {
        t.shown = visible;
    }
    #[cfg(not(target_os = "linux"))]
    let _ = visible;

    match effect {
        RenderModeEffect::None => None,
        RenderModeEffect::Freeze => {
            EngineManager::send_internal_cdp_with(
                next_cdp_id,
                &t.host,
                "Page.setWebLifecycleState",
                json!({ "state": "frozen" }),
            );
            Some(true)
        }
        RenderModeEffect::UnfreezeHidden => {
            EngineManager::send_internal_cdp_with(
                next_cdp_id,
                &t.host,
                "Page.setWebLifecycleState",
                json!({ "state": "active" }),
            );
            Some(false)
        }
        RenderModeEffect::Wake { visible, unfreeze } => {
            if unfreeze {
                EngineManager::send_internal_cdp_with(
                    next_cdp_id,
                    &t.host,
                    "Page.setWebLifecycleState",
                    json!({ "state": "active" }),
                );
            }
            t.awake.store(true, Ordering::Release);
            let fps = if crate::input_fps::is_boosted(id) {
                crate::input_fps::AGENT_INPUT_FPS.max(crate::input_fps::pane_fps())
            } else {
                crate::input_fps::pane_fps()
            };
            t.host.set_windowless_frame_rate(fps);
            t.host.was_hidden(0);
            if visible && focused {
                t.host.set_focus(1);
            }
            t.host.invalidate(PaintElementType::default());
            unfreeze.then_some(false)
        }
        RenderModeEffect::BecameVisible => {
            if focused {
                t.host.set_focus(1);
            }
            t.host.invalidate(PaintElementType::default());
            None
        }
        RenderModeEffect::BecameBackground => {
            if !t.is_popup {
                t.host.set_focus(0);
            }
            None
        }
        RenderModeEffect::Sleep => {
            // Gate render handler first so late in-flight Viz frames cannot reallocate GPU
            // slots or repopulate the desktop surface after release.
            t.awake.store(false, Ordering::Release);
            crate::input_fps::clear(id);
            t.host.set_focus(0);
            t.host.was_hidden(1);
            t.host.set_windowless_frame_rate(1);
            #[cfg(windows)]
            if let Some(ring) = &t.frames {
                ring.lock().unwrap().release_slots();
            }
            #[cfg(target_os = "linux")]
            if let Some(ring) = &t.frames {
                let _ = ring.lock().unwrap().step(crate::dmabuf_frames::Event::Released);
            }
            None
        }
    }
}

impl EngineManager {
    pub(super) fn on_hello_lifecycle(&mut self, tab_lifecycle: bool) {
        self.lifecycle_mode = if tab_lifecycle { LifecycleMode::Explicit } else { LifecycleMode::Legacy };
        self.unthrottled_tabs.clear();
        // Reset any previously frozen tabs so engine and newly connected host start in sync.
        let frozen_ids: Vec<u64> = self
            .tabs
            .iter()
            .filter_map(|(&id, t)| (t.render_mode == TabRenderMode::Hidden { frozen: true }).then_some(id))
            .collect();
        for id in frozen_ids {
            self.handle_freeze_tab(id, false);
        }
        if tab_lifecycle {
            self.visible_tabs.clear();
            self.sync_tab_visibility();
        }
    }

    pub(super) fn on_disconnect_lifecycle(&mut self) {
        self.unthrottled_tabs.clear();
        self.visible_tabs.clear();
        self.sync_tab_visibility();
    }

    pub(super) fn on_tab_closed_lifecycle(&mut self, tab_id: u64) {
        self.visible_tabs.remove(&tab_id);
        self.unthrottled_tabs.remove(&tab_id);
        if self.focused_tab == Some(tab_id) {
            self.focused_tab = None;
        }
        crate::input_fps::clear(tab_id);
    }

    /// Ensures `tab_id` is unfrozen before executing hidden navigation, reload, or CDP work.
    pub(super) fn ensure_tab_unfrozen(&mut self, tab_id: u64) {
        if self
            .tabs
            .get(&tab_id)
            .is_some_and(|t| t.render_mode == TabRenderMode::Hidden { frozen: true })
        {
            self.handle_freeze_tab(tab_id, false);
        }
    }

    /// Whether any managed tab is currently awake (used on macOS to relax the idle run-loop pump).
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    pub fn has_awake_tabs(&self) -> bool {
        self.tabs.values().any(|t| t.render_mode.is_awake())
    }

    pub(super) fn handle_set_visible_tabs(&mut self, tab_ids: Vec<u64>) {
        self.visible_tabs = tab_ids.into_iter().collect();
        self.sync_tab_visibility();
    }

    pub(super) fn handle_set_tab_unthrottled(&mut self, tab_id: u64, active: bool) {
        if active {
            self.unthrottled_tabs.insert(tab_id);
        } else {
            self.unthrottled_tabs.remove(&tab_id);
        }
        self.sync_tab_visibility();
    }

    pub(super) fn handle_freeze_tab(&mut self, tab_id: u64, frozen: bool) {
        let next_cdp_id = &self.next_cdp_id;
        let focused = self.focused_tab == Some(tab_id);
        let ext_change = if let Some(t) = self.tabs.get_mut(&tab_id) {
            let effect = step_render_mode(&mut t.render_mode, RenderModeEvent::SetFrozen(frozen));
            let notify = apply_render_effect(next_cdp_id, tab_id, t, false, focused, effect);
            notify.filter(|_| !t.isolated && !t.is_popup)
        } else {
            None
        };
        if let Some(is_frozen) = ext_change
            && let Some(ext) = &mut self.extensions
        {
            ext.set_tab_frozen(tab_id, is_frozen);
        }
    }

    /// Tier 3 hibernation: tears down `tab_id`'s CEF browser and renderer process while keeping
    /// completed agent downloads on disk, staged uploads, and `ExtensionHost` discarded tab state.
    pub(super) fn handle_hibernate_tab(&mut self, tab_id: u64) {
        let Some(tab) = self.tabs.remove(&tab_id) else {
            return;
        };
        self.on_tab_closed_lifecycle(tab_id);
        if self.prompt_shared.release_trace(tab_id) {
            self.send_internal_cdp(&tab.host, "Tracing.end", json!({}));
        }
        // Gate render handler and stop in-progress downloads/save dialogs without deleting completed files.
        tab.awake.store(false, Ordering::Release);
        tab.download_alive.store(false, Ordering::Release);
        self.downloads.cancel_pending_saves_for_tab(tab_id);
        if self.downloads.has_tab_files(tab_id) {
            let gens = self.hibernated_downloads.entry(tab_id).or_default();
            gens.extend(tab.prior_download_gens);
            gens.push(tab.download_gen);
        } else {
            self.downloads.close_tab(tab_id, tab.download_gen);
            for old_gen in tab.prior_download_gens {
                self.downloads.close_tab(tab_id, old_gen);
            }
        }
        self.release_preview(tab_id, tab.preview.as_ref());
        self.permissions.on_tab_closed(tab_id);
        tab.menu.cancel_all();
        tab.prompts.dismiss_all();
        tab.host.close_browser(1);
        self.release_tab_context(tab_id);
        if !tab.isolated
            && !tab.is_popup
            && let Some(ext) = &mut self.extensions
        {
            ext.on_tab_hibernated(tab_id);
        }
    }

    pub(super) fn close_managed_tab(&mut self, tab_id: u64) {
        self.on_tab_closed_lifecycle(tab_id);
        self.permissions.on_tab_closed(tab_id);
        let hibernated_gens = self.hibernated_downloads.remove(&tab_id);
        if let Some(tab) = self.tabs.remove(&tab_id) {
            if self.prompt_shared.release_trace(tab_id) {
                self.send_internal_cdp(&tab.host, "Tracing.end", json!({}));
            }
            self.release_preview(tab_id, tab.preview.as_ref());
            tab.menu.cancel_all();
            tab.prompts.dismiss_all();
            tab.download_alive.store(false, Ordering::Release);
            tab.host.close_browser(1);
            self.downloads.close_tab(tab_id, tab.download_gen);
            for old_gen in tab.prior_download_gens {
                self.downloads.close_tab(tab_id, old_gen);
            }
            if let Some(gens) = hibernated_gens {
                for old_gen in gens {
                    self.downloads.close_tab(tab_id, old_gen);
                }
            }
            crate::agent::remove_tab_uploads(&self.profile_dir, tab_id);
            self.release_tab_context(tab_id);
            if !tab.isolated
                && !tab.is_popup
                && let Some(ext) = &mut self.extensions
            {
                ext.on_tab_closed(tab_id);
            }
            let _ = self.out_tx.send(EngineMsg::TabClosed { tab_id });
        } else {
            if let Some(gens) = hibernated_gens {
                for old_gen in gens {
                    self.downloads.close_tab(tab_id, old_gen);
                }
            }
            crate::agent::remove_tab_uploads(&self.profile_dir, tab_id);
            if let Some(ext) = &mut self.extensions {
                ext.on_tab_closed(tab_id);
            }
            self.release_tab_context(tab_id);
        }
    }

    /// Cleans up a hibernated (discarded) tab closed by an extension (`chrome.tabs.remove`) and
    /// notifies the app via `EngineMsg::TabClosed`.
    pub(super) fn close_hibernated_tab_from_ext(&mut self, tab_id: u64) {
        let is_discarded = self.extensions.as_ref().is_some_and(|e| e.is_discarded_tab(tab_id));
        if !is_discarded {
            return;
        }
        let had_hibernated = self.hibernated_downloads.remove(&tab_id);
        self.on_tab_closed_lifecycle(tab_id);
        self.permissions.on_tab_closed(tab_id);
        if let Some(gens) = had_hibernated {
            for old_gen in gens {
                self.downloads.close_tab(tab_id, old_gen);
            }
        }
        crate::agent::remove_tab_uploads(&self.profile_dir, tab_id);
        self.release_tab_context(tab_id);
        if tab_id >= 1_000_000 {
            self.unmanaged_ext_tabs = self.unmanaged_ext_tabs.saturating_sub(1);
        }
        if let Some(ext) = &mut self.extensions {
            ext.on_tab_closed(tab_id);
        }
        let _ = self.out_tx.send(EngineMsg::TabClosed { tab_id });
    }

    pub(super) fn handle_focus_tab(&mut self, tab_id: u64) {
        let was_visible = self
            .tabs
            .get(&tab_id)
            .is_some_and(|t| t.render_mode == TabRenderMode::Visible);
        self.focused_tab = Some(tab_id);
        if self.lifecycle_mode == LifecycleMode::Legacy {
            self.visible_tabs.clear();
            self.visible_tabs.insert(tab_id);
        }
        for (&id, t) in &self.tabs {
            t.host.set_focus(i32::from(id == tab_id));
        }
        self.sync_tab_visibility();
        // If already visible, `sync_tab_visibility` emitted `None`; keep legacy repaint on FocusTab.
        if was_visible
            && let Some(t) = self.tabs.get(&tab_id)
            && t.render_mode == TabRenderMode::Visible
        {
            t.host.invalidate(PaintElementType::default());
        }
        self.on_tab_focus_extensions(tab_id);
    }

    pub(super) fn handle_ext_focus_tab(&mut self, engine_tab_id: u64) {
        self.focused_tab = Some(engine_tab_id);
        if self.lifecycle_mode == LifecycleMode::Legacy {
            self.visible_tabs.clear();
            self.visible_tabs.insert(engine_tab_id);
        }
        for (&id, t) in &self.tabs {
            t.host.set_focus(i32::from(id == engine_tab_id));
        }
        self.sync_tab_visibility();
    }

    pub(super) fn on_tab_opened(&mut self, tab_id: u64, focused: bool, is_popup: bool) {
        if is_popup {
            self.focused_tab = Some(tab_id);
            if let Some(t) = self.tabs.get(&tab_id) {
                t.host.was_hidden(0);
                t.host.set_focus(1);
            }
        } else if focused {
            self.focused_tab = Some(tab_id);
            if self.lifecycle_mode == LifecycleMode::Legacy {
                self.visible_tabs.clear();
                self.visible_tabs.insert(tab_id);
            }
            for (&id, existing) in &self.tabs {
                if !existing.is_popup {
                    existing.host.set_focus(i32::from(id == tab_id));
                }
            }
        } else if self.lifecycle_mode == LifecycleMode::Legacy {
            self.visible_tabs.remove(&tab_id);
        }
        self.sync_tab_visibility();
    }

    pub(super) fn sync_tab_visibility(&mut self) {
        let legacy = self.lifecycle_mode == LifecycleMode::Legacy;
        let focused_tab = self.focused_tab;
        let next_cdp_id = &self.next_cdp_id;
        let mut ext_frozen_updates: Vec<(u64, bool)> = Vec::new();
        for (&id, t) in &mut self.tabs {
            let visible = t.is_popup || self.visible_tabs.contains(&id);
            let held = self.unthrottled_tabs.contains(&id)
                || (legacy && t.prompts.agent_driven.load(Ordering::Relaxed));
            let effect = step_render_mode(&mut t.render_mode, RenderModeEvent::Sync { visible, held });
            if let Some(is_frozen) = apply_render_effect(next_cdp_id, id, t, visible, focused_tab == Some(id), effect)
                && !t.isolated
                && !t.is_popup
            {
                ext_frozen_updates.push((id, is_frozen));
            }
        }
        if !ext_frozen_updates.is_empty()
            && let Some(ext) = &mut self.extensions
        {
            for (id, is_frozen) in ext_frozen_updates {
                ext.set_tab_frozen(id, is_frozen);
            }
        }
    }
}
