//! Side, split, and bottom pane state mutations (open, close, select, move, toggle, reopen).

use std::collections::HashSet;

use rune::Tx;

use crate::closing::Event as ClosingEvent;
use crate::host::PanelsHost;
use crate::keys::run_closing;
use crate::tabs::{
    ClosedSideTab, SessionTabs, close_session_tab, move_session_tab, open_session_tab, place_session_tabs,
    preview_session_tab, push_closed_tab, session_tab_pane, split_panel_key, take_closed_tab,
};
use crate::{ClosedTabInfo, SessionPane};

use super::{
    BackgroundOpenEvent, CloseTabGuardEvent, NewTabEntry, NewTabFocus, SavedKeyboardFocus, SideMenu, Visibility,
    WidthHoldEvent, all_panels, find_panel, panes_shown, release_strip_width_holds, resolve_side_region,
    session_side_mut, shown_new_tab, side_open, side_stored_open,
};

/// Begins a background open (`BackgroundOpen::Active`) and snapshots current keyboard focus.
pub fn begin_keeping_keyboard(host: &mut dyn PanelsHost) {
    let composer_focused = host.composer_focused();
    let side = host.side_mut();
    let saved = SavedKeyboardFocus {
        composer_focused,
        side_focus: side.focus.mode(),
        active_pane_focus: side.active_pane_focus,
    };
    side.background_open.step(BackgroundOpenEvent::Begin(saved));
}

/// Ends the active background open and restores the focus snapshot saved by [`begin_keeping_keyboard`].
pub fn end_keeping_keyboard(host: &mut dyn PanelsHost) {
    let saved = {
        let side = host.side_mut();
        let Some(saved) = side.background_open.step(BackgroundOpenEvent::End) else {
            return;
        };
        side.focus.restore(saved.side_focus);
        side.active_pane_focus = saved.active_pane_focus;
        saved
    };
    host.set_composer_focused(saved.composer_focused);
}

/// Runs `open`, which shows a tab for someone other than the user (an agent's tool), leaving
/// the keyboard where it was: now, and when the tab's panel hears at the next poll that it
/// was selected, as a restored tab's does.
pub fn keeping_keyboard<H: PanelsHost, R>(host: &mut H, open: impl FnOnce(&mut H) -> R) -> R {
    begin_keeping_keyboard(host);
    let out = open(host);
    end_keeping_keyboard(host);
    out
}

/// Lands `tx` at once if showing a pane tab did not change which panes are visible (`was`);
/// else animates it, so the pane flies on `SIDE_SPRING` even from a poll, whose commit Rune
/// otherwise lands at once, and after the poll's later systems ([`super::flight`]). Callers
/// that may call `tx.instant()` after showing a tab (a scroll to a line) finish again with the
/// `was` they began with.
pub fn finish_show_pane_tab(host: &dyn PanelsHost, was: (bool, bool), tx: &mut Tx) {
    if panes_shown(host) == was {
        tx.instant();
        return;
    }
    tx.animated();
    super::flight::step(host, super::flight::PaneFlightEvent::Opened);
}

/// Lands a tab shown with `show` at once when its pane was already on screen, as switching
/// tabs does; a pane it opens flies on `SIDE_SPRING`, as the region's toggle does.
pub fn show_pane_tab<H: PanelsHost>(host: &mut H, tx: &mut Tx, show: impl FnOnce(&mut H)) {
    let was = panes_shown(host);
    show(host);
    finish_show_pane_tab(host, was, tx);
}

/// The session a panel of strip `strip` hears of: the routed one while that strip is on the page
/// (a subagent draws on its root's strip, and Desktop's panels get the routed session:
/// `panels.tsx` `panelInput(view)`), else the strip's own.
fn view_session(host: &dyn PanelsHost, strip: &str) -> String {
    match host.active_session_id() {
        Some(routed) if host.panel_session_id() == Some(strip) => routed.to_owned(),
        _ => strip.to_owned(),
    }
}

/// A subagent's own strip, as builds before its page showed its root's kept one: its tabs join
/// the strip its page shows now, once, after the root's own, so an update loses no tab. True
/// when it had any.
pub fn adopt_child_strip(host: &mut dyn PanelsHost, child: &str) -> bool {
    let root = host.panel_root(child).to_owned();
    if root == child || host.side().sessions.get(child).is_none_or(|own| own.tab_state.tabs.all.is_empty()) {
        return false;
    }
    let side = host.side_mut();
    let own = side.sessions.get_mut(child).map(|own| std::mem::take(&mut own.tab_state.tabs)).unwrap_or_default();
    let tabs = &mut side.sessions.entry(root).or_default().tab_state.tabs;
    for key in own.all {
        if !tabs.all.contains(&key) {
            if let Some(pane) = own.panes.get(&key) {
                tabs.panes.insert(key.clone(), *pane);
            }
            tabs.all.push(key);
        }
    }
    true
}

/// Sets the side region's open state: shared across sessions while `sharedSidePanel` is on
/// (`tabs.tsx` `setRegion`, #64950), otherwise per-session.
fn set_side_open(side: &mut super::SideState, shared: bool, sid: &str, open: bool) {
    if shared {
        side.shared_open = Some(open);
        if open {
            side.shared_mounted.insert(sid.to_owned());
        }
    } else {
        side.sessions.entry(sid.to_owned()).or_default().visibility.step(open);
    }
}

pub fn open_side_new_tab(host: &mut dyn PanelsHost, pane: SessionPane) -> String {
    host.wake_panel_launchers();
    let Some(sid) = host.panel_session_id().map(str::to_owned) else {
        return String::new();
    };
    let over = resolve_side_region(host).and_then(|reg| {
        let empty = SessionTabs::default();
        let stored = host.side().sessions.get(&sid).map_or(&empty, |s| &s.tab_state.tabs);
        reg.pane_active(pane, stored)
    });
    let shared = host.shared_side_panel();
    let id = {
        let side = host.side_mut();
        let token = side.next_new_serial;
        let id = format!("new:{token}");
        side.next_new_serial += 1;
        side.adding.push(NewTabEntry {
            id: id.clone(),
            pane,
            session_id: sid.clone(),
            over,
        });
        side.chosen_new_tab.insert(pane, id.clone());
        side.width_holds[pane as usize].step(WidthHoldEvent::Release);
        side.new_tab_focus = NewTabFocus::Requested { pane, token };
        side.focus.focus();
        side.active_pane_focus = pane;
        side.close_guard.step(CloseTabGuardEvent::RegionOpened);
        if pane == SessionPane::Bottom {
            side.sessions.entry(sid).or_default().bottom_open = true;
        } else {
            set_side_open(side, shared, &sid, true);
        }
        id
    };
    host.set_composer_focused(false);
    id
}

pub fn dismiss_side_new_tab(host: &mut dyn PanelsHost, new_id: &str) {
    let Some(sid) = host.panel_session_id().map(str::to_owned) else { return };
    host.side_mut().adding.retain(|n| n.id != new_id);
    // Desktop's `dismiss` empties the region only when the strip holds no tab in any pane and
    // no New tab is left (`new-tab-machine.ts`); with a bottom tab the side region stays open.
    let strip_empty = resolve_side_region(host).is_none_or(|r| r.strip.is_empty());
    let any_new = host.side().adding.iter().any(|n| n.session_id == sid);
    if strip_empty && !any_new {
        let shared = host.shared_side_panel();
        let side = host.side_mut();
        set_side_open(side, shared, &sid, false);
        side.focus.blur();
    }
}

/// Toggles the right-hand side region on its smooth spring (`⌥⌘B`, `⇧⌘R`, or header toggle).
pub fn toggle_side_region(host: &mut dyn PanelsHost, tx: &mut Tx) {
    host.wake_panel_launchers();
    let Some(sid) = host.panel_session_id().map(str::to_owned) else { return };
    host.side_mut().menu = SideMenu::Closed;
    let was_open = side_stored_open(host);
    let shared = host.shared_side_panel();
    if was_open {
        // Closing gives focus back to the prompt when it was in the region (`panels.tsx`
        // `toggle`'s `giveBackFocus`), rather than to nothing.
        // The toggle in the region's strip belongs to the session: its focus goes to the header's
        // copy that takes its place (`panels.tsx`, opencode#65308).
        if tx.focused() == Some("side-toggle") {
            tx.focus_when_shown("side-toggle", tx.focus_visible());
        } else if host.side().focus.is_focused() {
            tx.return_focus("prompt", tx.focus_visible());
        }
        {
            let side = host.side_mut();
            set_side_open(side, shared, &sid, false);
            side.focus.blur();
            side.new_tab_focus = NewTabFocus::Idle;
        }
        host.blur_panel_fields();
        return;
    }

    {
        let side = host.side_mut();
        let token = side.next_new_serial;
        side.next_new_serial += 1;
        set_side_open(side, shared, &sid, true);
        side.focus.focus();
        side.active_pane_focus = SessionPane::Side;
        side.new_tab_focus = NewTabFocus::Requested { pane: SessionPane::Side, token };
        side.close_guard.step(CloseTabGuardEvent::RegionOpened);
    }
    // Opening moves focus into it (`panels.tsx` `toggle`): the prompt gives up the keyboard
    // below, and without this nothing had it, so a reader heard nothing.
    tx.focus_when_shown("side-panel", tx.focus_visible());

    // An empty strip shows a New tab at once rather than on the next frame's poll.
    if sync_empty_side(host) {
        host.set_composer_focused(false);
        return;
    }
    let entered = resolve_side_region(host).and_then(|r| {
        let act_key = r.active.as_deref()?;
        let entry = r.entry(act_key)?;
        Some((entry.provider, entry.tab.id.clone()))
    });
    if let Some((provider, tab_id)) = entered {
        let view = view_session(host, &sid);
        provider.enter(host, &view, &tab_id);
    }
}

/// Desktop's New tab `sync` (`new-tab-machine.ts`): a side region open with nothing in its
/// strip, hidden tabs included, shows a New tab once per such span. That covers opening it
/// empty and its last tab leaving without closing it (a terminal that exits). A side region
/// whose only tabs are in the bottom pane shows the empty pane, as Desktop's does.
/// Whether the server hasn't listed the routed session yet (a subagent's page at launch): what a
/// plugin keeps for it can't be read, so the tabs it lists for it aren't known.
pub fn routed_unlisted(host: &dyn PanelsHost) -> bool {
    host.is_live() && host.active_session_id().is_some_and(|id| host.session_directory(id).is_none_or(str::is_empty))
}

pub fn sync_empty_side(host: &mut dyn PanelsHost) -> bool {
    let Some(id) = host.panel_session_id().map(str::to_owned) else { return false };
    let stored_empty = host.side().sessions.get(&id).is_none_or(|st| st.tab_state.tabs.all.is_empty());
    let candidate = side_stored_open(host) && stored_empty;
    if candidate && host.side().empty_span.within(&id) {
        return false;
    }
    // Pinned tabs fill the strip without being stored.
    let open_empty = candidate && resolve_side_region(host).is_none_or(|r| r.strip.is_empty());
    if !host.side().empty_span.moves(&id, open_empty) || !host.side_mut().empty_span.step(&id, open_empty) {
        return false;
    }
    let woke = host.wake_panel_launchers();
    // One left from an earlier opening shows again rather than being joined by another.
    if shown_new_tab(host, &id, SessionPane::Side, None).is_some() {
        return woke;
    }
    // Focus stays where it is, as Desktop's does: a terminal exiting doesn't take it.
    let side = host.side_mut();
    let new = NewTabEntry {
        id: format!("new:{}", side.next_new_serial),
        pane: SessionPane::Side,
        session_id: id,
        over: None,
    };
    side.next_new_serial += 1;
    side.chosen_new_tab.insert(SessionPane::Side, new.id.clone());
    side.adding.push(new);
    true
}

pub fn toggle_bottom_region(host: &mut dyn PanelsHost) {
    let Some(sid) = host.panel_session_id().map(str::to_owned) else { return };
    let side = host.side_mut();
    let st = side.sessions.entry(sid).or_default();
    st.bottom_open = !st.bottom_open;
    if st.bottom_open {
        side.focus.focus();
        side.active_pane_focus = SessionPane::Bottom;
        side.close_guard.step(CloseTabGuardEvent::RegionOpened);
    } else {
        side.focus.blur();
    }
}

/// Opens or focuses `key` (`"{plugin}:{tab_id}"`) in `session_id` (defaults to active session).
pub fn open_side_tab_for_session(
    host: &mut dyn PanelsHost,
    session_id: &str,
    key: &str,
    target_pane: Option<SessionPane>,
    preview: bool,
    first: bool,
) {
    // A child session's tabs are its root's (`PanelsHost::panel_root`).
    let strip = host.panel_root(session_id).to_owned();
    let session_id = strip.as_str();
    if let Some((plugin, _)) = split_panel_key(key) {
        host.ensure_panel_plugin(plugin);
    }
    let was = host
        .side()
        .sessions
        .get(session_id)
        .map_or((Visibility::Unseen, false), |st| (st.visibility, st.bottom_open));
    let transient_launchers: HashSet<String> = resolve_side_region(host)
        .map(|r| {
            r.entries
                .iter()
                .filter(|e| e.tab.transient)
                .map(|e| e.key.clone())
                .collect()
        })
        .unwrap_or_default();

    let default_pane = split_panel_key(key)
        .and_then(|(pl, _)| all_panels(host).into_iter().find(|p| p.plugin == pl))
        .map(|p| p.default_pane);
    let pane_choice = target_pane.or(default_pane);

    let was_shared = host.side().shared_open;
    let shared = host.shared_side_panel();
    let is_active = host.panel_session_id() == Some(session_id);
    {
        let side = host.side_mut();
        let actual_pane = {
            let st = side.sessions.entry(session_id.to_owned()).or_default();
            st.tab_state = if preview {
                preview_session_tab(&st.tab_state, key, &transient_launchers, pane_choice)
            } else {
                open_session_tab(&st.tab_state, key, &transient_launchers, first, pane_choice)
            };
            session_tab_pane(&st.tab_state.tabs, key)
        };
        if actual_pane == SessionPane::Bottom {
            side.sessions.entry(session_id.to_owned()).or_default().bottom_open = true;
        } else {
            set_side_open(side, shared, session_id, true);
        }
        side.width_holds[actual_pane as usize].step(WidthHoldEvent::Release);
        side.adding.retain(|n| !(n.session_id == session_id && n.pane == actual_pane));
        if matches!(side.new_tab_focus, NewTabFocus::Requested { pane, .. } if pane == actual_pane) {
            side.new_tab_focus = NewTabFocus::Idle;
        }
        side.focus.focus();
        side.active_pane_focus = actual_pane;
        if is_active && !side.background_open.is_active() {
            side.close_guard.step(CloseTabGuardEvent::RegionOpened);
        }
    }

    // The tab's panel and the narrow window's view hear of the session on the page.
    let view = view_session(host, session_id);
    if let Some((plugin, tab_id)) = split_panel_key(key)
        && let Some(prov) = find_panel(host, plugin)
    {
        prov.focus(host, &view, tab_id, false);
    }
    if host.place_narrow(&view, key, was) && shared {
        host.side_mut().shared_open = was_shared;
    }
}

/// Hides the pane showing `key` (Desktop's `layout.hide`): the bottom pane, or the side region.
pub fn hide_side_tab(host: &mut dyn PanelsHost, key: &str, tx: &mut Tx) {
    let Some(sid) = host.panel_session_id().map(str::to_owned) else { return };
    let is_bottom = host
        .side()
        .sessions
        .get(&sid)
        .is_some_and(|st| session_tab_pane(&st.tab_state.tabs, key) == SessionPane::Bottom);
    if is_bottom {
        let side = host.side_mut();
        side.sessions.entry(sid).or_default().bottom_open = false;
        side.focus.blur();
        return;
    }
    if side_stored_open(host) {
        toggle_side_region(host, tx);
    }
}

/// Desktop's `layout.open(key, session, { tab: "select" })`: adds `key` at the strip's end
/// when it isn't there, without replacing a preview tab, selects it, and shows its pane. A
/// new tab lands in `target`, else beside its plugin's last tab, else in its panel's default
/// pane.
pub fn show_side_tab(host: &mut dyn PanelsHost, session_id: &str, key: &str, target: Option<SessionPane>) {
    let strip = host.panel_root(session_id).to_owned();
    let session_id = strip.as_str();
    if let Some((plugin, _)) = split_panel_key(key) {
        host.ensure_panel_plugin(plugin);
    }
    let was = host
        .side()
        .sessions
        .get(session_id)
        .map_or((Visibility::Unseen, false), |st| (st.visibility, st.bottom_open));
    let fallback = split_panel_key(key)
        .and_then(|(pl, _)| all_panels(host).into_iter().find(|p| p.plugin == pl))
        .map(|p| p.home_pane(host));
    let prefix = split_panel_key(key).map(|(pl, _)| format!("{pl}:")).unwrap_or_default();

    let was_shared = host.side().shared_open;
    let shared = host.shared_side_panel();
    let is_active = host.panel_session_id() == Some(session_id);
    {
        let side = host.side_mut();
        let pane = {
            let st = side.sessions.entry(session_id.to_owned()).or_default();
            let sibling = st.tab_state.tabs.all.iter().any(|k| k.starts_with(&prefix));
            let mut next = st.tab_state.tabs.clone();
            if !next.all.iter().any(|k| k == key) {
                next.all.push(key.to_owned());
            }
            next.active = Some(key.to_owned());
            let target = target.or(if sibling { None } else { fallback });
            st.tab_state.tabs = place_session_tabs(&st.tab_state.tabs, next, target);
            session_tab_pane(&st.tab_state.tabs, key)
        };
        if pane == SessionPane::Bottom {
            side.sessions.entry(session_id.to_owned()).or_default().bottom_open = true;
        } else {
            set_side_open(side, shared, session_id, true);
        }
        side.adding.retain(|n| !(n.session_id == session_id && n.pane == pane));
        if is_active && !side.background_open.is_active() {
            side.close_guard.step(CloseTabGuardEvent::RegionOpened);
        }
    }

    // The tab's panel and the narrow window's view hear of the session on the page.
    let view = view_session(host, session_id);
    if let Some((plugin, tab_id)) = split_panel_key(key)
        && let Some(prov) = find_panel(host, plugin)
    {
        prov.focus(host, &view, tab_id, false);
    }
    if host.place_narrow(&view, key, was) && shared {
        host.side_mut().shared_open = was_shared;
    }
}

pub fn open_side_tab(
    host: &mut dyn PanelsHost,
    key: &str,
    target_pane: Option<SessionPane>,
    preview: bool,
    first: bool,
    _tx: &mut Tx,
) {
    let Some(sid) = host.active_session_id().map(str::to_owned) else { return };
    open_side_tab_for_session(host, &sid, key, target_pane, preview, first);
}

pub fn select_side_tab(host: &mut dyn PanelsHost, key: &str, tx: &mut Tx) {
    let Some(sid) = host.panel_session_id().map(str::to_owned) else { return };
    // Switching tabs is always instant (rules.md).
    tx.instant();
    host.side_mut().menu.close_tab_menu();
    // A plugin's field in the tab left behind gives up the keyboard, as the app's own do.
    host.blur_panel_fields();
    host.on_side_tab_selected(key);
    let pane = {
        let side = host.side_mut();
        let st = side.sessions.entry(sid.clone()).or_default();
        let p = session_tab_pane(&st.tab_state.tabs, key);
        st.tab_state.tabs.active = Some(key.to_owned());
        st.tab_state.tabs.selected.insert(p, key.to_owned());
        side.adding.retain(|n| !(n.session_id == sid && n.pane == p));
        if matches!(side.new_tab_focus, NewTabFocus::Requested { pane, .. } if pane == p) {
            side.new_tab_focus = NewTabFocus::Idle;
        }
        side.focus.focus();
        side.active_pane_focus = p;
        side.close_guard.step(CloseTabGuardEvent::RegionOpened);
        p
    };
    // A tab picked past the strip's edge, by a key or a click on its visible part, scrolls
    // into view, as Desktop's focused trigger does.
    tx.scroll_into_view_in(
        format!("side-tab-scroll-{}", pane.name()),
        format!("side-tab-{}", crate::view::safe_id(key)),
    );

    if let Some((plugin, tab_id)) = split_panel_key(key)
        && let Some(prov) = find_panel(host, plugin)
    {
        let view = view_session(host, &sid);
        prov.focus(host, &view, tab_id, false);
    }
}

pub fn move_side_tab(host: &mut dyn PanelsHost, key: &str, pane: SessionPane, before: Option<&str>, tx: &mut Tx) {
    let Some(sid) = host.panel_session_id().map(str::to_owned) else { return };
    tx.instant();
    let shared = host.shared_side_panel();
    release_strip_width_holds(host);
    let side = host.side_mut();
    side.close_guard.step(CloseTabGuardEvent::RegionOpened);
    {
        let st = side.sessions.entry(sid.clone()).or_default();
        st.tab_state.tabs = move_session_tab(&st.tab_state.tabs, key, pane, before);
        if pane == SessionPane::Bottom {
            st.bottom_open = true;
            return;
        }
    }
    set_side_open(side, shared, &sid, true);
}

pub fn promote_preview_tab(host: &mut dyn PanelsHost, key: &str) {
    let Some(sid) = host.panel_session_id().map(str::to_owned) else { return };
    if host
        .side()
        .sessions
        .get(&sid)
        .is_none_or(|st| st.tab_state.preview.as_deref() != Some(key))
    {
        return;
    }
    session_side_mut(host, &sid).tab_state.preview = None;
}

pub fn close_side_tab(host: &mut dyn PanelsHost, key: &str, tx: &mut Tx) {
    let Some(sid) = host.panel_session_id().map(str::to_owned) else { return };
    // The tab's panel hears of the session it drew for.
    let view = view_session(host, &sid);
    host.side_mut().menu.close_tab_menu();
    let reg = resolve_side_region(host);
    let entry = reg.as_ref().and_then(|r| r.entry(key)).cloned();
    if entry.as_ref().is_some_and(|e| !e.tab.can_close()) {
        return;
    }

    // Capture ClosedSideTab before removing the tab so the provider can still read its URL/state.
    let now_ms = host.now_ms();
    let closed_record = entry.as_ref().and_then(|e| {
        let info = e.provider.closed_info(host, &view, &e.tab);
        match info {
            ClosedTabInfo::Deny => None,
            ClosedTabInfo::Default if e.provider.transient || e.tab.transient => None,
            ClosedTabInfo::Default => Some(ClosedSideTab {
                key: key.to_owned(),
                title: e.tab.title.clone(),
                icon_name: None,
                file: e.tab.file.clone(),
                detail: None,
                data: None,
                closed_ms: now_ms,
            }),
            ClosedTabInfo::Allow(custom) => Some(ClosedSideTab {
                key: key.to_owned(),
                title: custom.title.unwrap_or_else(|| e.tab.title.clone()),
                icon_name: custom.icon_name,
                file: e.tab.file.clone(),
                detail: custom.detail,
                data: custom.data,
                closed_ms: now_ms,
            }),
        }
    });

    let was_pane = {
        let st = session_side_mut(host, &sid);
        if let Some(rec) = closed_record {
            push_closed_tab(&mut st.closed, rec);
        }
        let p = session_tab_pane(&st.tab_state.tabs, key);
        st.tab_state = close_session_tab(&st.tab_state, key);
        p
    };

    if let Some(e) = &entry {
        host.on_side_tab_closing(&view, key);
        e.provider.close(host, &view, &e.tab.id, tx);
    }

    // As Desktop's region `close`: the bottom pane shuts with its last tab, and the side region
    // only once the whole strip is empty with no New tab left in its side pane. A side pane
    // whose last tab closed while the bottom pane holds one stays open on the empty pane.
    let next_reg = resolve_side_region(host);
    let strip_empty = next_reg.as_ref().is_none_or(|r| r.strip.is_empty());
    let side_new = host.side().adding.iter().any(|n| n.session_id == sid && n.pane != SessionPane::Bottom);
    let any_bottom = next_reg.as_ref().is_some_and(|r| r.strip.iter().any(|e| e.pane == SessionPane::Bottom));
    if was_pane == SessionPane::Bottom && !any_bottom {
        let side = host.side_mut();
        side.sessions.entry(sid.clone()).or_default().bottom_open = false;
        side.adding.retain(|n| !(n.session_id == sid && n.pane == SessionPane::Bottom));
        side.focus.blur();
    }
    if strip_empty && !side_new && side_stored_open(host) {
        // The side region slides shut on its spring; a side tab it closed stays on screen
        // until it lands.
        let at = host.clock();
        let shared = host.shared_side_panel();
        let effects = {
            let side = host.side_mut();
            set_side_open(side, shared, &sid, false);
            side.focus.blur();
            entry
                .filter(|_| was_pane != SessionPane::Bottom)
                .map(|entry| side.closing.step(ClosingEvent::LastTabClosed { session_id: sid, entry: Box::new(entry), at }))
        };
        if let Some(effects) = effects {
            run_closing(effects, tx);
        }
        return;
    }
    if was_pane == SessionPane::Bottom {
        return;
    }
    // Remaining tabs close the gap at once (docs/kit-design.md: immediate).
    tx.instant();
    let next_focus = next_reg
        .as_ref()
        .and_then(|r| r.active.as_deref())
        .and_then(|k| split_panel_key(k).map(|(pl, tid)| (pl.to_owned(), tid.to_owned())));
    if let Some((pl, tid)) = next_focus
        && let Some(prov) = find_panel(host, &pl)
    {
        prov.focus(host, &view, &tid, false);
    }
}

pub fn reopen_closed_side_tab(host: &mut dyn PanelsHost, key: Option<&str>, tx: &mut Tx) -> bool {
    let Some(sid) = host.panel_session_id().map(str::to_owned) else {
        return false;
    };
    if host.side().sessions.get(&sid).is_none_or(|st| st.closed.is_empty()) {
        return false;
    }
    let Some(closed) = take_closed_tab(&mut session_side_mut(host, &sid).closed, key) else {
        return false;
    };
    release_strip_width_holds(host);
    host.side_mut().close_guard.step(CloseTabGuardEvent::RegionOpened);
    if side_open(host) {
        tx.instant();
    }
    let view = view_session(host, &sid);
    if let Some((plugin, tab_id)) = split_panel_key(&closed.key)
        && let Some(prov) = find_panel(host, plugin)
        && prov.restore(host, &view, tab_id, closed.data.as_deref(), tx)
    {
        return true;
    }
    open_side_tab_for_session(host, &sid, &closed.key, None, false, false);
    true
}
