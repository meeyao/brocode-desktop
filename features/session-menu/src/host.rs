//! Object-safe host boundary and pure-data DTOs for `opencode-feature-session-menu`.
//!
//! Every state machine, menu builder, action dispatcher, key handler, and poll entrypoint in this
//! crate operates on `&dyn SessionMenuHost` / `&mut dyn SessionMenuHost`. Thin generic bounds
//! (`H: SessionMenuHost + 'static`) appear strictly at `rune::El` view/callback boundaries and
//! delegate in 1 line to non-generic functions.

use std::path::PathBuf;

use opencode_kit::client::{Server, Write};
use opencode_kit::model::{SessionInfo, Transcript};
use opencode_kit::servers::ServerKey;
use opencode_kit::wire_error::WireError;
use opencode_ui::popup::{Menu, PopupAnchor};
use rune::{El, FileDialogOptions, Tx};
use serde_json::Value;
use opencode_ui::palette::Pal;

use crate::Menus;

/// What became of a write offered quietly (`SessionMenuHost::write_server_quiet`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Offered {
    /// Handed to the server's writer.
    Sent,
    /// The window has no writer for it: no server, or one whose writer isn't attached yet.
    NoWriter,
    /// The build may not send.
    ReadOnly,
}

/// A session's activity and unread status across its family (`useSessionTabAvatarState`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SessionMarks {
    /// A permission or form waits on the user somewhere in the session's family.
    pub attention: bool,
    /// Something in the family is working, and nothing waits on the user.
    pub busy: bool,
    /// Waiting on the user, or finished since it was last read.
    pub unread: bool,
}

pub use opencode_runtime::TabSummary;

/// Callback invoked when a session context menu closes with the picked item ID (or `None`).
pub type MenuPick = Box<dyn FnOnce(&mut dyn SessionMenuHost, Option<&str>, &mut Tx)>;

/// Deferred UI-thread mutation delivered from a background task (such as session export).
pub type HostAction = Box<dyn FnOnce(&mut dyn SessionMenuHost, &mut Tx) + Send>;

/// Thread-safe handle for scheduling a [`HostAction`] back onto the UI thread.
pub type HostDeliverer = Box<dyn Fn(HostAction) + Send + 'static>;

/// Callback invoked once a native save-file dialog completes (`Ok(None)` when cancelled).
pub type SaveFileDone = Box<dyn FnOnce(&mut dyn SessionMenuHost, Result<Option<PathBuf>, String>, &mut Tx) + Send>;

/// Object-safe host boundary for session context menus, inline/dialog rename, undoable delete,
/// session export, manual unread marks, and per-session hide/mute preferences.
pub trait SessionMenuHost: opencode_kernel::AppEnv + opencode_kernel::AppOverlays {
    /// Session menu state (tracked read).
    fn menus(&self) -> &Menus;
    /// Session menu state (tracked write — callers only invoke when mutating).
    fn menus_mut(&mut self) -> &mut Menus;

    /// Loaded sessions on the currently checked-out server.
    fn sessions(&self) -> &[SessionInfo] {
        &[]
    }
    /// Mutable access to loaded sessions on the currently checked-out server (only invoked when mutating).
    fn sessions_mut(&mut self) -> Option<&mut Vec<SessionInfo>> {
        None
    }
    /// Looks up session `id` on the currently checked-out server.
    fn session(&self, id: &str) -> Option<&SessionInfo> {
        self.sessions().iter().find(|s| s.id == id)
    }
    /// Loaded transcript for session `id` on the currently checked-out server, if available.
    fn transcript(&self, id: &str) -> Option<&Transcript> {
        let _ = id;
        None
    }
    /// Root session ID for `id` (or `id` itself for root/unknown sessions).
    fn root_of<'a>(&'a self, id: &'a str) -> &'a str {
        id
    }
    /// Family activity and unread marks for root session `root_id`.
    fn session_marks(&self, root_id: &str) -> SessionMarks {
        let _ = root_id;
        SessionMarks::default()
    }

    /// Open window tabs in strip order.
    fn tabs(&self) -> Vec<TabSummary> {
        Vec::new()
    }
    /// The tabs the strip shows, in its order: what Close other tabs and Close tabs below act on
    /// (official's `visibleTabs`, `shell/titlebar/tab-strip.tsx:946-954`). Every open tab by
    /// default.
    fn strip_tabs(&self) -> Vec<TabSummary> {
        self.tabs()
    }
    /// Whether the active tab layout renders tabs down a sidebar (affects "Close tabs below" wording).
    fn layout_has_sidebar(&self) -> bool {
        false
    }
    /// Whether project/recent navigation (pin & mute items in menus) is enabled.
    fn project_navigation(&self) -> bool {
        false
    }
    /// Whether Consolidated navigation prototype is active (pin & group items lead tab menus).
    fn navigation_prototype(&self) -> bool {
        false
    }
    /// Whether the side panel region currently holds keyboard focus (used to avoid stealing Ctrl+R).
    fn side_focused(&self) -> bool {
        false
    }
    /// Whether a Storybook stage is mounted over the window (writes settle immediately).
    fn stage_active(&self) -> bool {
        false
    }

    /// Server key the window is currently checked out on.
    fn active_server(&self) -> ServerKey {
        ServerKey::sidecar()
    }
    /// Whether the server runtime boot phase has settled (`Boot::Ready`).
    fn boot_ready(&self) -> bool {
        true
    }
    /// Live server client on the currently checked-out server, or `None` for fixtures.
    fn live_server(&self) -> Option<&Server> {
        None
    }
    /// Whether `server` had session `id` when this window last checked it out.
    fn checkout_known(&self, server: &ServerKey, id: &str) -> bool {
        let _ = (server, id);
        false
    }
    /// Cached read-menu state (`Some(true)` -> mark read, `Some(false)` -> mark unread) for a parked server.
    fn checkout_read_menu(&self, server: &ServerKey, id: &str) -> Option<bool> {
        let _ = (server, id);
        None
    }
    /// Returns `(title, project_id)` for session `id` on `server` when known, for the Copy submenu
    /// (`packages/app/src/session/session-copy-menu.tsx`, `opencode#66576`).
    fn session_copy_info(&self, server: &ServerKey, id: &str) -> Option<(String, String)> {
        if *server == self.active_server() {
            return self.session(id).map(|i| (i.title.clone(), i.project.clone()));
        }
        None
    }
    /// Runs `f` checked out on a parked `server`, restoring the previous checkout afterwards.
    /// Returns `false` if this window has no half of `server`. (Active-server calls run directly
    /// via `on_server` without needing this hook.)
    fn with_server(&mut self, server: &ServerKey, f: &mut dyn FnMut(&mut dyn SessionMenuHost)) -> bool {
        let _ = (server, f);
        false
    }
    /// [`Self::write_server`] without the host's own word when the write can't go: the caller's
    /// machine says so itself, from what this hands back.
    fn write_server_quiet(&mut self, write: Write) -> Offered {
        if self.write_server(write) { Offered::Sent } else { Offered::NoWriter }
    }
    /// Sends a session write (`Rename`, `Delete`, `View`, ...) to the currently checked-out server.
    fn write_server(&mut self, write: Write) -> bool {
        let _ = write;
        false
    }

    /// Reports root session `root` viewed up to its latest idle mark when it's unread
    /// (`notification.session.markViewed`).
    fn mark_viewed(&mut self, root: &str) {
        let Some(idle) = self.session(root).filter(|info| info.unread()).and_then(|info| info.idle) else { return };
        if let Some(info) = self.sessions_mut().and_then(|sessions| sessions.iter_mut().find(|i| i.id == root)) {
            info.viewed = Some(idle);
        }
        self.write_server(Write::View { session: root.to_owned(), idle });
    }

    /// Display label for `server` (used in error formatting and labels).
    fn server_label(&self, server: &ServerKey) -> String {
        server.as_str().to_owned()
    }
    /// Formats a typed [`WireError`] into a single-line human message for the active server.
    fn format_wire_error(&self, what: &str, error: &WireError) -> String {
        let _ = what;
        error.to_string()
    }

    /// Opens a native or drawn popup menu at `anchor` and forwards the picked item ID to `on_pick`.
    fn open_popup(&mut self, menu: Menu, anchor: PopupAnchor, tx: &mut Tx, on_pick: MenuPick) {
        drop((menu, anchor, tx, on_pick));
    }
    /// Draws a static preview surface of `menu` with item index `active` lit (for Storybook stills).
    fn popup_surface(&self, p: &Pal, menu: &Menu, active: Option<usize>) -> El {
        let _ = (p, menu, active);
        rune::el()
    }

    /// Shows a failure toast.
    fn toast_fail(&mut self, title: &str, description: &str) {
        let _ = (title, description);
    }
    /// Shows a success toast.
    fn toast_succeed(&mut self, title: &str, description: &str) {
        let _ = (title, description);
    }
    /// Shows a structured wire-error toast for a failed write reply.
    fn toast_wire_error(&mut self, error: WireError) {
        let _ = error;
    }
    /// The same toast under `title`, for a write its call doesn't name: a session's PATCH
    /// renames, archives and restores.
    fn toast_wire_error_titled(&mut self, title: &str, error: WireError) {
        let _ = (title, error);
    }
    /// Offers an undoable delete toast; when the toast expires or the app quits, the host calls
    /// [`crate::send_delete`], or [`crate::undelete`] if the user presses Undo.
    fn offer_delete_undo(&mut self, title: &str, server: ServerKey, session: String, hidden: Vec<SessionInfo>) {
        let _ = (title, server, session, hidden);
    }

    /// Whether persistent storage is hydrated and ready to read/write.
    fn storage_ready(&self) -> bool {
        false
    }
    /// Reads a JSON value from `(namespace, key)` in persistent storage.
    fn storage_read_json(&self, namespace: &str, key: &str) -> Option<Value> {
        let _ = (namespace, key);
        None
    }
    /// Persists a JSON value under `(namespace, key)` in persistent storage.
    fn storage_write_json(&mut self, namespace: &str, key: &str, value: &Value) {
        let _ = (namespace, key, value);
    }
    /// Notifies peer windows that shared unread or preference state changed.
    fn touch_peers(&mut self) {}

    /// Blurs competing text editors (composer, home search) when inline or dialog rename takes focus.
    fn on_rename_focus_taken(&mut self) {}
    /// Persists updated tab metadata (titles, removed tabs).
    fn save_tabs(&mut self) {}
    /// Reads session `id` from the server again.
    fn refresh_session(&mut self, id: &str) {
        let _ = id;
    }
    /// Opens session `id` on `server`.
    fn open_session(&mut self, server: &ServerKey, id: &str, tx: &mut Tx) {
        let _ = (server, id, tx);
    }
    /// Leading rows the host adds to a session tab's menu (Consolidated's Pin & Add to group).
    fn pin_menu_extra(&self, _server: &ServerKey, _id: &str) -> Vec<crate::Line> {
        Vec::new()
    }
    /// Runs a row [`SessionMenuHost::pin_menu_extra`] added; `false` for another id.
    fn run_pin_menu_extra(&mut self, _server: &ServerKey, _id: &str, _menu_id: &str, _tx: &mut Tx) -> bool {
        false
    }
    /// Rows the host adds to tab `uid`'s menu (Tab groups' Add to group); none by default.
    fn tab_menu_extra(&self, _uid: &str) -> Vec<crate::Line> {
        Vec::new()
    }
    /// Runs a row [`SessionMenuHost::tab_menu_extra`] added; `false` for another id.
    fn run_tab_menu_extra(&mut self, _uid: &str, _menu_id: &str, _tx: &mut Tx) -> bool {
        false
    }
    /// Closes window tab `uid`.
    fn close_tab(&mut self, uid: &str, tx: &mut Tx) {
        let _ = (uid, tx);
    }
    /// The window tab the open page belongs to (its `uid`), if any.
    fn shown_tab(&self) -> Option<String> {
        None
    }
    /// Shows window tab `uid`.
    fn show_tab(&mut self, uid: &str, tx: &mut Tx) {
        let _ = (uid, tx);
    }
    /// Whether `(server, id)` is currently pinned in the Recent sidebar.
    fn is_session_pinned(&self, server: &ServerKey, id: &str) -> bool {
        let _ = (server, id);
        false
    }
    /// Toggles pin state for `(server, id)` in the Recent sidebar.
    fn toggle_session_pin(&mut self, server: &ServerKey, id: &str) {
        let _ = (server, id);
    }
    /// Opens session `id` on `server` as a tab behind the page that shows (Home row menu's
    /// "Open in background", upstream #65209/#65212).
    fn open_session_in_background(&mut self, server: &ServerKey, id: &str) {
        let _ = (server, id);
    }
    /// Archives session `id` on `server`.
    fn archive_session(&mut self, server: &ServerKey, id: &str, tx: &mut Tx) {
        let _ = (server, id, tx);
    }
    /// Unarchives session `id` on `server`.
    fn unarchive_session(&mut self, server: &ServerKey, id: &str) {
        let _ = (server, id);
    }
    /// Duplicates/forks session `id` on `server`.
    fn duplicate_session(&mut self, server: &ServerKey, id: &str, tx: &mut Tx) {
        let _ = (server, id, tx);
    }
    /// Snapshots the inbox's visible row order before a delete hides or removes sessions
    /// (`packages/app/src/shell/titlebar/inbox.tsx:212-248`, `opencode#66558`).
    fn record_inbox_shown(&mut self) {}
    /// Cleans up tabs, navigation history, closed tabs, and active route after `server` confirms
    /// sessions `gone` were deleted (the session list itself has already been pruned).
    fn on_sessions_removed(&mut self, server: &ServerKey, gone: &[String], tx: &mut Tx) {
        let _ = (server, gone, tx);
    }
    /// Handles a server reply to an `archive` session write: a refusal puts the session back.
    fn on_archive_replied(&mut self, session: String, ok: bool, tx: &mut Tx) {
        let _ = (session, ok, tx);
    }
    /// Handles a server reply to an `unarchive` session write (e.g. switcher restore toast).
    fn on_unarchive_replied(&mut self, session: String, ok: bool, why: &str, tx: &mut Tx) {
        let _ = (session, ok, why, tx);
    }
    /// Returns a thread-safe deliverer for posting background task completions back onto the UI thread.
    fn deliverer(&self) -> Option<HostDeliverer> {
        None
    }
    /// Opens a native save-file dialog and delivers the chosen path (or cancellation/error) to `on_done`.
    fn save_file(&mut self, options: FileDialogOptions, tx: &mut Tx, on_done: SaveFileDone) {
        drop((options, tx, on_done));
    }
}

/// Runs `f` checked out on `server`, returning `None` if `host` has no half of `server`.
pub(crate) fn on_server<R>(
    host: &mut dyn SessionMenuHost,
    server: &ServerKey,
    f: impl FnOnce(&mut dyn SessionMenuHost) -> R,
) -> Option<R> {
    if *server == host.active_server() {
        return Some(f(host));
    }
    let mut slot = Some(f);
    let mut out = None;
    let ran = host.with_server(server, &mut |h| {
        if let Some(cb) = slot.take() {
            out = Some(cb(h));
        }
    });
    ran.then_some(out).flatten()
}
