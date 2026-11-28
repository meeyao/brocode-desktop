//! Object-safe host boundary for the session side, split, and bottom panels feature.

use opencode_ui::keymap::Chord;
use rune::{El, Ime, KeyEvent, Tx};
use rune_knobs::Knobs;
use opencode_ui::palette::Pal;

use crate::state::{SideState, Visibility};
use crate::{ClosedTabInfo, DockPlacement, PanelLauncherDesc, PanelRegistration, PanelSessionCtx, PanelTabDesc, PanelViewCtx, SessionPane};

/// Window chrome metrics consumed by the side region, header toggle, and resize handle.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PanelChrome {
    /// Top inset under the window titlebar row.
    pub top: f32,
    /// Bottom inset above a bottom-mounted narrow titlebar.
    pub foot: f32,
    /// Fingerprint of the full window chrome for `rune::region_with` invalidation.
    pub stamp: u64,
}

/// Object-safe host boundary for the session side, split, and bottom panel system.
///
/// Every state machine, resolver, action, key/IME handler, and poll entrypoint in
/// `opencode-feature-panels` operates on `&dyn PanelsHost` / `&mut dyn PanelsHost`.
/// Generic `H: PanelsHost + 'static` bounds appear only at `rune::El` view/callback
/// boundaries, delegating 1-line to non-generic handlers.
pub trait PanelsHost: opencode_kernel::AppEnv {
    /// Side/split/bottom panel state (tracked read).
    fn side(&self) -> &SideState;
    /// Side/split/bottom panel state (tracked write — callers only invoke when mutating).
    fn side_mut(&mut self) -> &mut SideState;

    /// Working directory of session `sid`, if loaded.
    fn session_directory(&self, sid: &str) -> Option<&str>;
    /// Project ID of session `sid` (empty if global or unknown).
    fn session_project_id(&self, sid: &str) -> String {
        let _ = sid;
        String::new()
    }
    /// Whether session `sid` currently shows its session header (and thus the top-right
    /// side-region toggle button). False while a missing/error state panel replaces the session.
    fn session_has_header(&self, sid: &str) -> bool {
        let _ = sid;
        true
    }

    /// Whether the settings screen currently covers the session route.
    fn settings_open(&self) -> bool {
        false
    }
    /// Whether the window is below Desktop's narrow (768 px) breakpoint.
    fn narrow(&self) -> bool {
        false
    }
    /// Whether a Storybook stage is currently mounted over the window.
    fn stage_active(&self) -> bool {
        false
    }
    /// Currently shown left sidebar width in zoomed window points.
    fn sidebar_shown_w(&self) -> f32 {
        0.0
    }
    /// Default pane a new dockable panel (e.g. terminal) opens in (`Settings › Terminal`).
    fn dock_placement(&self) -> DockPlacement {
        DockPlacement::Side
    }
    /// Whether the side panel open state is shared across every session tab (`general.sharedSidePanel`, #64950).
    fn shared_side_panel(&self) -> bool {
        false
    }
    /// Window chrome metrics for positioning and caching the side region.
    fn panel_chrome(&self) -> PanelChrome {
        PanelChrome { top: 8.0, foot: 0.0, stamp: 0 }
    }
    /// Live look-and-feel knobs (used for popover placement and motion).
    fn knobs(&self) -> &Knobs;

    /// Whether the session composer currently holds keyboard focus (tracked read).
    fn composer_focused(&self) -> bool {
        false
    }
    /// Whether the session composer holds keyboard focus without subscribing the caller's
    /// tracked region to composer keystrokes.
    fn composer_focused_untracked(&self) -> bool {
        self.composer_focused()
    }
    /// Updates whether the session composer holds keyboard focus.
    fn set_composer_focused(&mut self, focused: bool) {
        let _ = focused;
    }
    /// Whether an active popup overlay currently holds keyboard focus.
    fn popup_holds_focus(&self) -> bool {
        false
    }
    /// Whether a text field belonging to `plugin` currently holds keyboard focus.
    fn plugin_field_focused(&self, plugin: &str) -> bool {
        let _ = plugin;
        false
    }
    /// Blurs any focused plugin text fields (called when switching tabs or closing the side region).
    fn blur_panel_fields(&mut self) {}

    /// Effective chord for published command `id` (respecting user overrides).
    fn command_chord(&self, id: &str) -> Option<Chord> {
        let _ = id;
        None
    }

    /// The strokes published command `id`'s labels show: more than one for a chord
    /// (opencode#64395), none when unbound.
    fn command_strokes(&self, id: &str) -> Vec<Chord> {
        self.command_chord(id).into_iter().collect()
    }

    /// Resolves a stored closed-tab icon without waking dormant plugin VMs.
    fn resolve_closed_icon(&self, key: &str, name: Option<&str>) -> &'static rune::Icon {
        let _ = key;
        crate::icon_by_name(name)
    }

    /// Monotonic version of plugin-contributed panel surfaces for region memoization.
    fn plugin_surfaces_version(&self) -> u64 {
        0
    }
    /// Stamp of host inputs that can affect stored-tab rekeying or transient tab pruning
    /// (active session, plugin surfaces, side state, native panels, sessions, projects, debug).
    fn panel_tabs_stamp(&self) -> u64 {
        self.plugin_surfaces_version()
    }
    /// Every registered panel provider (sorted by `(order, plugin)` by [`crate::state::all_panels`]).
    fn panel_providers(&self) -> Vec<PanelRegistration> {
        Vec::new()
    }
    /// Lists the tabs offered by `ctx.plugin` for `ctx.session_id`.
    fn panel_list(&self, ctx: &PanelSessionCtx<'_>) -> Vec<PanelTabDesc> {
        let _ = ctx;
        Vec::new()
    }
    /// Renders `ctx.tab` inside its pane.
    fn panel_render(&self, p: &Pal, ctx: &PanelViewCtx<'_>) -> El {
        let _ = (p, ctx);
        rune::el()
    }
    /// Whether any panel of `plugin` requests the wider session minimum right now.
    fn panel_is_wide(&self, plugin: &str) -> bool {
        let _ = plugin;
        false
    }
    /// Lists the `+` / `"New tab"` launcher entries offered by `ctx.plugin`.
    fn panel_launchers(&self, ctx: &PanelSessionCtx<'_>) -> Vec<PanelLauncherDesc> {
        let _ = ctx;
        Vec::new()
    }
    /// Renders plugin slots on the side panel's New tab view (`session.panel.new`, `session.panel.new.list`).
    fn panel_new_tab_slots(
        &self,
        p: &Pal,
        slot: &str,
        session_id: &str,
        pane: SessionPane,
        width: f32,
        focus_token: Option<u64>,
    ) -> Vec<El> {
        let _ = (p, slot, session_id, pane, width, focus_token);
        Vec::new()
    }
    /// Runs launcher entry `id` of `plugin` in `pane`.
    fn open_panel_entry(&mut self, plugin: &str, id: &str, pane: SessionPane, tx: &mut Tx) {
        let _ = (plugin, id, pane, tx);
    }
    /// Ensures `plugin` is running before opening or showing one of its tabs (waking a dormant or
    /// deferred built-in synchronously so the tab strip never opens empty).
    fn ensure_panel_plugin(&mut self, plugin: &str) {
        let _ = plugin;
    }
    /// Handles pointer hover over launcher entry `id` of `plugin`.
    fn hover_panel_entry(&mut self, plugin: &str, id: &str, tx: &mut Tx) {
        let _ = (plugin, id, tx);
    }
    /// Notifies `plugin` that `tab` of `session` became selected or entered.
    fn panel_focused(&mut self, plugin: &str, session: &str, tab: &str, restored: bool) {
        let _ = (plugin, session, tab, restored);
    }
    /// Notifies `plugin` that `tab` left `session`'s strip.
    fn panel_closed(&mut self, plugin: &str, session: &str, tab: &str, tx: &mut Tx) {
        let _ = (plugin, session, tab, tx);
    }
    /// Returns what `"Recently closed"` should record for `tab` as it closes.
    fn panel_closed_info(&self, plugin: &str, session: &str, tab: &PanelTabDesc) -> ClosedTabInfo {
        let _ = (plugin, session, tab);
        ClosedTabInfo::Default
    }
    /// Restores a closed tab via `plugin`'s custom handler; returns `false` to fall back to
    /// reopening the tab key directly.
    fn panel_restore(&mut self, plugin: &str, session: &str, tab: &str, data: Option<&str>, tx: &mut Tx) -> bool {
        let _ = (plugin, session, tab, data, tx);
        false
    }
    /// Returns a custom tab strip label element for `tab`, or `None` to draw the standard icon + title.
    fn panel_tab_lead(&self, p: &Pal, plugin: &str, tab: &PanelTabDesc, selected: bool) -> Option<El> {
        let _ = (p, plugin, tab, selected);
        None
    }
    /// Returns a version stamp for `tab`'s custom strip label (`None` when using the standard label).
    fn panel_tab_lead_stamp(&self, p: &Pal, plugin: &str, tab: &PanelTabDesc, selected: bool) -> Option<u64> {
        let _ = (p, plugin, tab, selected);
        None
    }
    /// Extra horizontal width inside a custom strip label beyond icon + title (e.g. a 6 pt status dot + 6 pt gap).
    fn panel_tab_extra_w(&self, plugin: &str, tab: &PanelTabDesc) -> f32 {
        let _ = (plugin, tab);
        0.0
    }
    /// Returns the preferred home pane for a new tab of `plugin`, overriding `default_pane` when `Some`.
    fn panel_home_pane(&self, plugin: &str) -> Option<SessionPane> {
        let _ = plugin;
        None
    }
    /// Wakes dormant plugins that contribute panel launchers; returns `true` if any woke.
    fn wake_panel_launchers(&mut self) -> bool {
        false
    }

    /// Forwards an IME event to the active native panel surface.
    fn handle_panel_ime(&mut self, ime: &Ime) -> bool {
        let _ = ime;
        false
    }
    /// Handles plugin-registered panel command shortcuts (e.g. Toggle terminal, Open review).
    fn handle_panel_command_key(&mut self, ev: &KeyEvent, tx: &mut Tx) -> bool {
        let _ = (ev, tx);
        false
    }
    /// Handles scoped key events for active native panel views.
    fn handle_panel_key(&mut self, ev: &KeyEvent, tx: &mut Tx) -> bool {
        let _ = (ev, tx);
        false
    }

    /// Places a newly opened tab into the narrow-window view switcher instead of opening side/bottom
    /// panes, restoring the previous `(visibility, bottom_open)` state when narrow. Returns `true`
    /// when narrow mode intercepted the open.
    fn place_narrow(&mut self, session_id: &str, key: &str, was: (Visibility, bool)) -> bool {
        let _ = (session_id, key, was);
        false
    }
    /// Called when `key` is selected in the tab strip (e.g. to blur a file filter input for non-file tabs).
    fn on_side_tab_selected(&mut self, key: &str) {
        let _ = key;
    }
    /// Called right before `panel_closed` when `key` is closing (e.g. to release file preview/watch state).
    fn on_side_tab_closing(&mut self, session_id: &str, key: &str) {
        let _ = (session_id, key);
    }
    /// External URL of browser panel tab `id`, for the tab context menu's Open/Copy actions.
    fn browser_tab_external_url(&self, id: u64) -> Option<String> {
        let _ = id;
        None
    }
    /// Opens browser panel tab `id` in the system browser.
    fn open_browser_tab_externally(&mut self, id: u64, tx: &mut Tx) {
        let _ = (id, tx);
    }
}
