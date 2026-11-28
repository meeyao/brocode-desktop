//! Built-ins that start at their first use. A built-in this launch's first screen doesn't draw,
//! with a record of what it contributed when it last started, stays `Dormant` after the first
//! frame: its commands, keybinds, menu rows, panels' launchers, settings search rows and MCP tools
//! are listed from the record, and no VM runs for it. Its first use starts it in that same turn
//! and the use goes on against what it registers live ([`wake`]):
//!
//! - **At once:** one of its commands, keybinds, slash commands, menu or app-menu rows, code block
//!   actions or MCP tools runs; a link is opened (link handlers answer `match`); a session's
//!   screen it draws on is routed to ([`wake_for_route`]); the palette, the slash list or a menu
//!   whose rows it answers for opens ([`wake_kind`]).
//! - **Ahead of a use:** a session row is hovered or highlighted ([`near_session`]): the ones a
//!   session draws start one per turn, none while input waits.
//! - **When idle:** the window has had no input for [`crate::idle::AFTER`] and none waits; one per
//!   turn. Background duties (the updater's check and pill, Keep screen active) and anything a surface on screen
//!   asked of a dormant plugin ([`want`]) start this way.
//!
//! Nothing is missed meanwhile: the server events it listened for ([`Record::events`]) are kept
//! while it's dormant ([`buffer_event`]) and handed to it, in order, as it starts; everything else
//! a plugin mirrors (sessions, the route, stores) it reads as it starts, as a plugin started after
//! the first frame always has.
//!
//! The records are one state row of the boot record (`RECORDS`), read with the rest of it before the
//! first frame: a built-in is dormant from the first frame. Each is keyed by the plugin's own files'
//! hash, a constant `build.rs` makes (`sources::files_hash`), and the interface language: a built-in
//! whose record doesn't match (an update that changed it, another language, a first launch) starts
//! after the first frame as before, and records itself.

use std::collections::BTreeMap;
use std::time::Duration;

pub use opencode_plugin_runtime::dormant::{Asleep, Record, Wait, Why};
use opencode_plugin_runtime::dormant::{Records, RecordsEffect, RecordsEvent, contribs_of, parse_records, record_kinds};
use opencode_plugin_spec as spec;
use rune::Tx;

use super::plugin::Contribs;
use crate::App;
use crate::plugin_host::{Life, LifeEvent, Overlay, PluginHost, Tier, bump, drive, tasks};

/// Server events kept for one dormant plugin; the oldest go first past it.
const KEPT_EVENTS: usize = 256;

/// The window's dormant plugins and the records file as this window read it.
#[derive(Default)]
pub struct Dormant {
    pub asleep: BTreeMap<String, Asleep>,
    /// Every record read or made this launch; written back when one changed.
    records: Records,
    /// Events kept for a plugin that started, handed to it at the next poll.
    pub catch_up: BTreeMap<String, Vec<(String, serde_json::Value)>>,
    /// Wake requests queued by views of this window since the last poll.
    asked: std::cell::RefCell<Vec<(String, Wait)>>,
    /// Count of settings-open retries for bundled settings plugins killed by a startup watchdog.
    watchdog_retries: BTreeMap<String, u8>,
}

impl Dormant {
    pub(super) fn ensure_read(&mut self) {
        if self.records.is_unread() {
            let found = read_file();
            self.records.step(RecordsEvent::Loaded(found));
        }
    }

    /// [`Dormant::record`] of records already read ([`Dormant::ensure_read`]).
    pub fn kept(&self, id: &str, hash: &str) -> Option<&Record> {
        let locale = opencode_i18n::locale().id();
        self.records.get(id).filter(|r| r.hash == hash && r.locale == locale)
    }

    /// The record of built-in `id` for `hash`, in the current language.
    pub fn record(&mut self, id: &str, hash: &str) -> Option<Record> {
        self.ensure_read();
        let locale = opencode_i18n::locale().id();
        self.records.get(id).filter(|r| r.hash == hash && r.locale == locale).cloned()
    }
}

/// Where the records are: a state row in a namespace the boot record carries whole.
const RECORDS: (&str, &str) = (crate::storage::DEV, "plugins.contributions");

/// `record`'s `session.panel` rows.
fn panel_rows(record: &Record) -> impl Iterator<Item = &opencode_plugin_runtime::dormant::Entry> {
    record.window.get("menu_items").into_iter().flatten().filter(|e| e.spec.get("menu").and_then(|v| v.as_str()) == Some("session.panel"))
}

/// Whether the plugin of `record` has to run for a session's New tab view to list its entries as
/// it would: it has one there that answers a function (a row's `when`, a panel's `enabled`). A
/// plugin with no entry there needn't, nor one whose entries are all in its record: of fifteen
/// built-ins a side panel restored on New tab started before its first frame, four have such an
/// entry (strobe `rec_7ecdd3`, `rec_5a45c9`, `rec_2fd572`: 18.5 ms of plugin starts).
fn is_new_tab_slot(e: &opencode_plugin_runtime::dormant::Entry) -> bool {
    matches!(e.spec.get("at").and_then(|v| v.as_str()), Some("session.panel.new" | "session.panel.new.list"))
}

pub(super) fn launcher_needs(record: &Record) -> bool {
    let asks = |e: &opencode_plugin_runtime::dormant::Entry, name: &str| e.fns.iter().any(|f| f == name);
    let panel = record.window.get("panels").is_some_and(|es| es.iter().any(|e| !asks(e, "list") && asks(e, "enabled") && e.spec.get("launcher").and_then(|v| v.as_bool()) != Some(false)));
    let new_tab_slot = record.window.get("slots").is_some_and(|es| es.iter().any(is_new_tab_slot));
    panel || new_tab_slot || panel_rows(record).any(|e| asks(e, "when"))
}
/// The records as written last: none when there are none yet, they don't read, or are of another
/// version. Before the first frame this is the boot packet's, so it reads no file.
fn read_file() -> Option<BTreeMap<String, Record>> {
    let text = crate::storage::text(RECORDS.0, RECORDS.1)?;
    Some(parse_records(&text))
}

/// The file a build before the records were in the database kept beside it, and its leftover: they
/// go off the app's thread, as the first records are written. Once the database has them, this
/// never runs again.
fn forget_old_file() {
    let Some(cache) = crate::dirs::cache() else { return };
    tasks::run(move || {
        for name in ["plugin-contributions.json", "plugin-contributions.json.tmp"] {
            std::fs::remove_file(cache.join(name)).ok();
        }
    });
}

/// Notes what plugin `i` contributed as it started, for its next launch's record.
pub fn note_started(app: &mut App, i: usize) {
    let p = &app.plugins.list[i];
    let Some(hash) = super::sources::files_hash(&p.manifest.id, &p.origin, p.tier) else { return };
    let id = p.manifest.id.clone();
    let app_half = super::app_half::each(|a| (*a.half.id == *id).then(|| record_kinds(&a.half.contribs.borrow(), &["menubar_items", "mcp_tools"]))).into_iter().next().unwrap_or_default();
    let vm = p.script().and_then(rune_luau::Script::vm_id);
    let mut signals: Vec<String> = p
        .half
        .signals
        .borrow()
        .keys()
        .filter(|(v, name)| Some(*v) == vm && !name.starts_with("ctx.") && !name.starts_with("editor."))
        .map(|(_, name)| name.clone())
        .collect();
    signals.sort();
    signals.dedup();
    let record = Record {
        hash,
        locale: opencode_i18n::locale().id().to_owned(),
        window: record_kinds(&p.half.contribs.borrow(), &[]),
        app: app_half,
        events: p.half.events.borrow().iter().map(|(_, prefix, _)| prefix.clone()).collect(),
        signals,
    };
    let d = &mut app.plugins.dormant;
    d.ensure_read();
    d.records.step(RecordsEvent::Note { id, record });
}

/// Writes the records back, when one changed (the storage's writer commits it off the app's thread).
pub fn save(app: &mut App) {
    // Read before it's taken: a write to the host rebuilds every region showing it.
    if !app.plugins.dormant.records.is_dirty() {
        return;
    }
    if let Some(RecordsEffect::Write { json, forget_old_file: clean_old }) = app.plugins.dormant.records.step(RecordsEvent::Save) {
        crate::storage::set(RECORDS.0, RECORDS.1, json);
        if clean_old {
            forget_old_file();
        }
    }
}

/// Puts plugin `i` to sleep with `record`: its contributions come from it until it starts.
pub fn doze(app: &mut App, i: usize, record: Record, background: bool) {
    let p = &app.plugins.list[i];
    let id = p.manifest.id.clone();
    super::sources::intern_icons(&id, &p.origin.files());
    *p.half.contribs.borrow_mut() = contribs_of(&record.window);
    p.half.epoch.set(p.half.epoch.get() + 1);
    let wait = if background { Wait::Idle } else { Wait::FirstUse };
    app.plugins.dormant.asleep.insert(id, Asleep { record, wait, ..Asleep::default() });
    drive(app, i, LifeEvent::Doze);
    let id = app.plugins.list[i].manifest.id.clone();
    let app_half = contribs_of(&app.plugins.dormant.asleep[&id].record.app);
    APP_HALVES.with_borrow_mut(|a| a.push((id, app_half)));
}

/// A window at rest showing no session (`App::rest`): each built-in a use started goes back to
/// sleep with the record that start wrote, as it is at launch, so its commands, keys and menus stay
/// listed and its next use starts it again. Not one with work of its own (`background`), nor one
/// busy now: a process or run of it, power or a prompt it holds, its dialog or popover open. Not
/// with another window open either, which may show it and shares its app half.
pub fn sleep_unused(app: &mut App) {
    if app.other_windows() || app.settings_open() {
        return;
    }
    let running: Vec<(usize, String)> = app.plugins.list.iter().enumerate()
        .filter(|(_, p)| p.tier == Tier::Bundled && matches!(p.life, Life::Running(_)) && !super::boot::background(&p.manifest.id))
        .map(|(i, p)| (i, p.manifest.id.clone()))
        .collect();
    for (i, id) in running {
        if busy(app, i, &id) {
            continue;
        }
        let p = &app.plugins.list[i];
        let Some(hash) = super::sources::files_hash(&id, &p.origin, p.tier) else { continue };
        let Some(record) = app.plugins.dormant.record(&id, &hash) else { continue };
        drive(app, i, LifeEvent::Sleep);
        super::app_half::stop(&id);
        doze(app, i, record, false);
        eprintln!("plugins: {id} sleeps again (nothing shows it)");
    }
}

/// Whether plugin i is doing something a stop would end.
fn busy(app: &App, i: usize, id: &str) -> bool {
    let half = &app.plugins.list[i].half;
    let overlay = match &app.plugins.overlay {
        Overlay::Popover { plugin, .. } | Overlay::PillMenu { plugin, .. } => &**plugin == id,
        Overlay::Closed => false,
    };
    half.procs.is_busy() || !half.held.borrow().is_empty()
        || app.plugins.dialog.as_ref().is_some_and(|d| &*d.plugin == id) || overlay
        || super::app_half::each(|a| (&*a.half.id == id).then(|| a.half.procs.is_busy() || !a.half.held.borrow().is_empty())).into_iter().any(|b| b)
}

/// One window's dormant app half `id` is gone (it started, was turned off, or its window closed).
fn release_app_half(id: &str) {
    APP_HALVES.with_borrow_mut(|a| {
        if let Some(at) = a.iter().position(|(owner, _)| owner == id) {
            a.remove(at);
        }
    });
}

/// The window is closing: its dormant plugins' app halves are no longer listed for it.
pub fn close(host: &PluginHost) {
    for id in host.dormant.asleep.keys() {
        release_app_half(id);
    }
}

thread_local! {
    /// The app halves of dormant plugins, as their records list them: their app menu rows and MCP
    /// tools, which the app lists without a window (`commands::menubar_items`).
    static APP_HALVES: std::cell::RefCell<Vec<(String, Contribs)>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// Runs `f` on each dormant app half's recorded contributions, once per plugin, but for those whose
/// app half runs (another window started it): the running one lists them.
pub fn each_app_half<R>(mut f: impl FnMut(&str, &Contribs) -> Option<R>) -> Vec<R> {
    let running: Vec<String> = super::app_half::each(|a| Some(a.half.id.to_string()));
    APP_HALVES.with_borrow(|a| {
        let mut seen: Vec<&str> = Vec::new();
        let mut out = Vec::new();
        for (id, c) in a {
            if running.contains(id) || seen.contains(&id.as_str()) {
                continue;
            }
            seen.push(id);
            out.extend(f(id, c));
        }
        out
    })
}

/// Debug-only scratch-root hold used to keep `updater` dormant while proving the Settings fallback.
fn debug_held(id: &str) -> bool {
    #[cfg(debug_assertions)]
    {
        id == "updater"
            && std::env::var_os("OPENCODE_RUNE_HOLD_UPDATER_DORMANT").is_some()
            && std::env::var_os("RUNE_DIRS_ROOT").is_some()
    }
    #[cfg(not(debug_assertions))]
    {
        let _ = id;
        false
    }
}

/// Starts plugin `id` now, for `why`, if it's dormant; true when it was and runs now.
pub fn wake(app: &mut App, id: &str, why: Why) -> bool {
    if debug_held(id) {
        return false;
    }
    let Some(i) = app.plugins.find(id).filter(|&i| matches!(app.plugins.list[i].life, Life::Dormant)) else { return false };
    let started = rune::time::Instant::now();
    let asleep = app.plugins.dormant.asleep.remove(id).unwrap_or_default();
    release_app_half(id);
    *app.plugins.list[i].half.contribs.borrow_mut() = Contribs::default();
    drive(app, i, LifeEvent::Load);
    // The events kept for it, but for those still waiting for this poll: it hears those with the rest.
    let pending = app.plugins.events.borrow().clone();
    let kept: Vec<_> = asleep.events.into_inner().into_iter().filter(|e| !pending.contains(e)).collect();
    let caught = kept.len();
    if !kept.is_empty() {
        app.plugins.dormant.catch_up.entry(id.to_owned()).or_default().extend(kept);
    }
    let running = app.plugins.list[i].running();
    let when = match why { Why::Use => "first use", Why::Near => "next turn (a session near, or a surface showing it)", Why::Idle => "idle time" };
    eprintln!("plugins: started {id} at its {when} in {} us; {caught} server events kept while it slept go to it", started.elapsed().as_micros());
    rune::marks::mark(Box::leak(format!("plugin.{id}.woke").into_boxed_str()));
    bump();
    running
}

/// Plugin `id` was turned off while dormant: its recorded contributions go with it.
pub fn forget(app: &mut App, id: &str) {
    if app.plugins.dormant.asleep.remove(id).is_some()
        && let Some(half) = app.plugins.half(id)
    {
        *half.contribs.borrow_mut() = Contribs::default();
        release_app_half(id);
    }
}

/// Wakes, now, each dormant plugin whose recorded contributions `owns` picks; true when one woke.
pub fn wake_owners(app: &mut App, owns: impl Fn(&Contribs) -> bool) -> bool {
    let ids: Vec<String> = app.plugins.dormant.asleep.keys().filter(|id| app.plugins.half(id).is_some_and(|h| owns(&h.contribs.borrow()))).cloned().collect();
    let app_ids: Vec<String> = each_app_half(|id, c| owns(c).then(|| id.to_owned()));
    let mut any = false;
    for id in ids.into_iter().chain(app_ids) {
        any |= wake(app, &id, Why::Use);
    }
    any
}

/// Wakes, now, each dormant plugin whose recorded signals contain `signal`; true when one woke.
pub fn wake_signal(app: &mut App, signal: &str) -> bool {
    let ids: Vec<String> = app
        .plugins
        .dormant
        .asleep
        .iter()
        .filter(|(_, a)| a.record.signals.iter().any(|s| s == signal))
        .map(|(id, _)| id.clone())
        .collect();
    let mut any = false;
    for id in ids {
        any |= wake(app, &id, Why::Use);
    }
    any
}

/// The palette, the slash list or a menu is opening: any queued built-ins contributing to `kind`
/// are adopted at once, and dormant plugins whose rows in it answer a function (`enabled`,
/// `hidden`, `when`) start, so the rows it lists are complete even on a first launch.
pub fn wake_kind(app: &mut App, kind: &str, fns: &[&str]) -> bool {
    super::boot::doze_owed(app);
    let adopted = match kind {
        "commands" => super::boot::adopt_later_kinds(app, &["Command"]),
        "menu_items" => super::boot::adopt_later_kinds(app, &["MenuItem"]),
        _ => false,
    };
    let ids: Vec<String> = app.plugins.dormant.asleep.iter().filter(|(_, a)| a.record.window.get(kind).is_some_and(|es| es.iter().any(|e| e.fns.iter().any(|f| fns.contains(&f.as_str()))))).map(|(id, _)| id.clone()).collect();
    let mut woke = false;
    for id in ids {
        woke |= wake(app, &id, Why::Use);
    }
    adopted || woke
}

/// Settings is opening: adopts any queued built-ins contributing commands or settings surfaces,
/// wakes dormant ones, and retries any bundled settings plugin killed by a startup watchdog so
/// Settings › Preferences, Shortcuts and plugin pages are complete immediately.
pub fn wake_settings(app: &mut App) -> bool {
    super::boot::doze_owed(app);
    let adopted = super::boot::adopt_later_kinds(app, &["Command", "Setting", "SettingsPage"]);
    let woke = wake_owners(app, |c| !c.settings.is_empty() || !c.settings_pages.is_empty());
    let retried = retry_timed_out_settings(app);
    adopted || woke || retried
}

/// Retries bundled settings plugins that hit a transient startup watchdog timeout (at most once
/// per launch), so a CPU burst during boot never permanently drops a built-in Settings section.
/// It runs beside the general restart (`lifecycle::restart_later`, one restart 2 s after a timed-out
/// start): this one comes later, when Settings opens, which still matters where the watchdog's
/// budget is wall time (Windows on ARM, web).
fn retry_timed_out_settings(app: &mut App) -> bool {
    const MAX_RETRIES: u8 = 1;
    let ids: Vec<(usize, String)> = app
        .plugins
        .list
        .iter()
        .enumerate()
        .filter(|(_, p)| {
            p.tier == Tier::Bundled
                && !app.plugins.disabled.contains(&p.manifest.id)
                && matches!(&p.life, Life::Errored { error, .. } if error.contains("watchdog"))
                && super::sources::contributions(&p.manifest.id).iter().any(|k| matches!(*k, "Setting" | "SettingsPage"))
                && app.plugins.dormant.watchdog_retries.get(&p.manifest.id).copied().unwrap_or(0) < MAX_RETRIES
        })
        .map(|(i, p)| (i, p.manifest.id.clone()))
        .collect();
    let mut any = false;
    for (i, id) in ids {
        let n = app.plugins.dormant.watchdog_retries.entry(id.clone()).or_insert(0);
        *n += 1;
        eprintln!("plugins: retrying {id} on Settings open after startup watchdog (attempt {n}/{MAX_RETRIES})");
        drive(app, i, LifeEvent::Load);
        any = true;
    }
    any
}

/// Whether dormant plugin `id`'s recorded contribution `key` of `kind` handed over function `name`.
pub fn recorded_fn(app: &App, id: &str, kind: &str, key: &str, name: &str) -> bool {
    app.plugins.dormant.asleep.get(id).and_then(|a| a.record.window.get(kind)).is_some_and(|es| es.iter().any(|e| e.key == key && e.fns.iter().any(|f| f == name)))
}

/// The side region's New tab view or its `+` menu is opening: any queued or dormant plugins with
/// entries there (panels, `session.panel` rows) start, so the entries are the ones they offer.
pub fn wake_launchers(app: &mut App) -> bool {
    let adopted = super::boot::adopt_later_kinds(app, &["Panel", "MenuItem", "Slot"]);
    // A plugin whose entries there are all in its record (`launcher_needs`) stays asleep: the
    // view lists them from it (`panels::asleep_launchers`), and opening one starts it. A launch
    // reaches this before its first frame, for a side panel saved on New tab
    // (`panels::sync_empty_side`).
    let ids: Vec<String> = app.plugins.dormant.asleep.iter().filter(|(_, a)| launcher_needs(&a.record)).map(|(id, _)| id.clone()).collect();
    let mut woke = false;
    for id in ids {
        woke |= wake(app, &id, Why::Use);
    }
    adopted || woke
}

/// A surface on screen asked dormant plugin `id` something only its VM answers: it starts in idle
/// time. Called from views, so it only marks and nudges the window's waker for the next poll.
pub fn want(app: &App, id: &str) {
    if wants(app, id, Wait::Idle) {
        let first = {
            let mut w = app.plugins.dormant.asked.borrow_mut();
            let empty = w.is_empty();
            push_once(&mut w, id, Wait::Idle);
            empty
        };
        if first && let Some(wake) = &*app.waker {
            wake();
        }
    }
}

/// [`want`] for a surface that shows dormant plugin `id`'s part now (a slot, a code block, a file
/// view, or an open New tab launcher list): it starts at the next turn, so the surface draws
/// without it for one frame at most.
pub fn want_now(app: &App, id: &str) {
    if wants(app, id, Wait::Next) {
        let first = {
            let mut w = app.plugins.dormant.asked.borrow_mut();
            let empty = w.is_empty();
            push_once(&mut w, id, Wait::Next);
            empty
        };
        if first && let Some(wake) = &*app.waker {
            wake();
        }
    }
}

/// Whether asking dormant plugin `id` to start at `to` moves it.
fn wants(app: &App, id: &str, to: Wait) -> bool {
    // The debug plugin's slot is its bar, which draws nothing while it's hidden.
    if id == "debug" && !super::first_screen::debug_bar_shown() {
        return false;
    }
    app.plugins.dormant.asleep.get(id).is_some_and(|a| a.wait < to)
}

fn push_once(list: &mut Vec<(String, Wait)>, id: &str, to: Wait) {
    match list.iter_mut().find(|(x, _)| x == id) {
        Some((_, w)) => *w = (*w).max(to),
        None => list.push((id.to_owned(), to)),
    }
}

/// Whether session `session`'s side region is open on New tab (empty side strip, or adding a tab).
pub(super) fn shows_launchers(app: &App, session: &str) -> bool {
    let side_state = app.kernel.slots.get(&crate::panels::SIDE);
    // A subagent's page draws its root's strip (`App::panel_root`).
    let session = app.panel_root(session);
    let st = side_state.sessions.get(session);
    let side_open = if app.settings.shared_side_panel {
        side_state.shared_open.unwrap_or(false)
    } else {
        st.is_some_and(|st| st.visibility.open())
    };
    if !side_open {
        return false;
    }
    let has_side_tab = st.is_some_and(|st| {
        st.tab_state.tabs.all.iter().any(|k| st.tab_state.tabs.panes.get(k).copied().unwrap_or(crate::panels::SessionPane::Side) != crate::panels::SessionPane::Bottom)
    });
    !has_side_tab || side_state.adding.iter().any(|n| n.session_id == session)
}

/// The dormant plugins a session's screen draws, as `first_screen` decides for a launch: its
/// header slots (the debug bar's only while shown), its composer pills, the plugins with a tab in
/// the session, and the file viewers when it has a Files tab. A code block a plugin draws asks for
/// it as it shows ([`want_now`]); a link handler answers when a link is opened.
fn for_session(app: &App, session: &str) -> Vec<String> {
    let side = app.kernel.slots.get(&crate::panels::SIDE).sessions.get(app.panel_root(session));
    let with_tabs: Vec<&str> = side.map(|side| side.tab_state.tabs.all.iter().filter_map(|k| crate::panels::tabs::split_panel_key(k).map(|(plugin, _)| plugin)).collect()).unwrap_or_default();
    let files = with_tabs.contains(&"file");
    let on_new_tab = shows_launchers(app, session);
    let has = |a: &Asleep, kind: &str| a.record.window.get(kind).is_some_and(|es| !es.is_empty());

    app.plugins
        .dormant
        .asleep
        .iter()
        .filter(|(id, a)| {
            let non_new_tab_slot = a.record.window.get("slots").is_some_and(|es| es.iter().any(|e| !is_new_tab_slot(e)));
            let slot = non_new_tab_slot && (id.as_str() != "debug" || super::first_screen::debug_bar_shown());
            let launcher = on_new_tab && launcher_needs(&a.record);
            slot || has(a, "composer_pills") || with_tabs.contains(&id.as_str()) || (files && has(a, "file_viewers")) || launcher
        })
        .map(|(id, _)| id.clone())
        .collect()
}

/// The window routes to session `session`: what its screen draws starts before its frame.
pub fn wake_for_route(app: &mut App, session: &str) {
    if shows_launchers(app, session) {
        super::boot::adopt_later_kinds(app, &["Panel", "MenuItem"]);
    }
    for id in for_session(app, session) {
        wake(app, &id, Why::Use);
    }
}

/// A session row was hovered or highlighted: what a session's screen draws starts ahead of the
/// click, one per turn ([`poll`]).
pub fn near_session(app: &App, session: &str) {
    let mut asked = app.plugins.dormant.asked.borrow_mut();
    for id in for_session(app, session) {
        push_once(&mut asked, &id, Wait::Next);
    }
}

/// The sleeping plugins with a composer pill start from the next turn, one per turn: a launch
/// leaves them out of its first frame (`first_screen`). True when one was asked for.
pub fn want_pills(app: &App) -> bool {
    let mut asked = app.plugins.dormant.asked.borrow_mut();
    let ids: Vec<&String> = app.plugins.dormant.asleep.iter().filter(|(_, a)| a.wait < Wait::Next && a.record.window.get("composer_pills").is_some_and(|es| !es.is_empty())).map(|(id, _)| id).collect();
    for id in &ids {
        push_once(&mut asked, id, Wait::Next);
    }
    !ids.is_empty()
}

/// Keeps server event `ty` for each dormant plugin that listened for it.
pub fn buffer_event(app: &App, ty: &str, data: &serde_json::Value) {
    for a in app.plugins.dormant.asleep.values().filter(|a| a.record.events.iter().any(|p| ty.starts_with(p.as_str()))) {
        let mut events = a.events.borrow_mut();
        if events.len() >= KEPT_EVENTS {
            events.pop_front();
        }
        events.push_back((ty.to_owned(), data.clone()));
    }
}

/// Whether a dormant plugin listened for server event `ty`.
pub fn listens(app: &App, ty: &str) -> bool {
    app.plugins.dormant.asleep.values().any(|a| a.record.events.iter().any(|p| ty.starts_with(p.as_str())))
}

/// Each turn: one dormant plugin starts, none while input waits; a near one at once, others only
/// once the window has had no input for [`crate::idle::AFTER`]. Asks to be woken when idle time comes.
pub fn poll(app: &mut App, tx: &mut Tx) -> bool {
    // Borrowed through `&App`, so taking an empty queue does not mark the plugin host changed.
    let asked = std::mem::take(&mut *app.plugins.dormant.asked.borrow_mut());
    for (id, to) in asked {
        if let Some(a) = app.plugins.dormant.asleep.get_mut(&id) {
            a.wait = a.wait.max(to);
        }
    }
    // A launch trace's hold (`--launch-trace-hold`): nothing sleeping starts until it's over.
    if let Some(left) = crate::launch::trace::held() {
        if app.plugins.dormant.asleep.values().any(|a| a.wait > Wait::FirstUse) {
            tx.wake_after(left);
        }
        return false;
    }
    if tx.input_waiting() {
        if app.plugins.dormant.asleep.iter().any(|(id, a)| a.wait == Wait::Next && !debug_held(id)) {
            tx.wake_after(Duration::ZERO);
        } else if app.plugins.dormant.asleep.iter().any(|(id, a)| a.wait == Wait::Idle && !debug_held(id)) {
            tx.wake_after(crate::idle::AFTER);
        }
        return false;
    }
    let due = |wait: Wait| app.plugins.dormant.asleep.iter().find(|(id, a)| a.wait == wait && !debug_held(id)).map(|(id, _)| id.clone());
    if let Some(id) = due(Wait::Next) {
        wake(app, &id, Why::Near);
        if app.plugins.dormant.asleep.iter().any(|(id, a)| a.wait > Wait::FirstUse && !debug_held(id)) {
            tx.wake_after(Duration::ZERO);
        }
        return true;
    }
    let Some(id) = due(Wait::Idle) else { return false };
    let quiet = crate::idle::quiet_for();
    if quiet < crate::idle::AFTER {
        tx.wake_after(crate::idle::AFTER - quiet);
        return false;
    }
    // In pieces, one a turn: its app half first, then the rest (`wake`), so a key that comes
    // meanwhile waits for one piece, not the whole start.
    if super::lifecycle::start_app_half(app, &id) {
        tx.wake_after(Duration::ZERO);
        return false;
    }
    wake(app, &id, Why::Idle);
    if app.plugins.dormant.asleep.iter().any(|(id, a)| a.wait > Wait::FirstUse && !debug_held(id)) {
        tx.wake_after(Duration::ZERO);
    }
    true
}

/// The language changed: records in the old one name things in it, so every dormant plugin starts
/// in idle time and records itself again.
pub fn locale_changed(app: &mut App) {
    for a in app.plugins.dormant.asleep.values_mut() {
        a.wait = a.wait.max(Wait::Idle);
    }
}
