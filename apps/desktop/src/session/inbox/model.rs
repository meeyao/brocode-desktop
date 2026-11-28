//! Desktop's Inbox layout (`shell/titlebar/inbox-model.ts`, `inbox.tsx`, `inbox-working.ts`,
//! `inbox-branch-machine.ts`): every root session of every listed server with the state a person
//! acts on, sorted into the sidebar's groups. A plain projection of each server's session store:
//! the server the window works against read live (`App::sessions`, and the family's requests and
//! work from `session_status`), the others as of their last checkout (`snap`, which the runtime
//! runs as it leaves each). The inbox keeps only its own view state here.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use opencode_kit::model::{Project, SessionInfo};
use opencode_kit::servers::ServerKey;
use rune::Tx;

use crate::layout_state::TabLayout;
use crate::{App, Route, Source};

/// Recent shows this many before Show more (`RECENT_LIMIT`).
pub const RECENT_LIMIT: usize = 10;

/// What a root session needs from a person, across its subagent family.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum InboxStatus {
    /// A permission waits on you.
    Approval,
    /// A question or form waits on you.
    Input,
    /// Something in the family runs.
    Working,
    /// It finished, failed, and nobody has looked yet.
    Failed,
    /// It finished and nobody has looked yet.
    Done,
    /// Read and idle.
    Ready,
}

/// One root session in the inbox.
#[derive(Clone, Copy, Debug)]
pub struct Entry<'a> {
    /// The server it's on.
    pub server: &'a ServerKey,
    pub session: &'a SessionInfo,
    active_ms: i64,
    pub status: InboxStatus,
    /// Wall-clock ms the row's age counts from.
    pub since: Option<i64>,
    /// Its project, on its server.
    pub project: Option<&'a Project>,
    /// It runs in a worktree of its project, not the project's own folder.
    pub worktree: bool,
    /// Its folder's git branch, once read.
    pub branch: Option<&'a str>,
}

impl Entry<'_> {
    /// The row's identity across servers (`${server}\0${session.id}`), as tabs name theirs.
    pub fn key(&self) -> String {
        crate::tabs::session_uid(self.server, &self.session.id)
    }

    /// Waiting on a person: a permission to decide or a question to answer (`inboxWaiting`,
    /// `inbox-model.ts`).
    pub fn waiting(&self) -> bool {
        matches!(self.status, InboxStatus::Approval | InboxStatus::Input)
    }
}

/// A root session of a server the window isn't checked out on, as of its last checkout.
#[derive(Clone, Debug)]
pub(super) struct Row {
    pub(super) session: SessionInfo,
    status: InboxStatus,
    since: Option<i64>,
    project: Option<Project>,
    worktree: bool,
    /// Its folder's branch, once its server's `Branches` read it.
    branch: Option<String>,
}

/// The sidebar's groups, in their order (`inbox.tsx` `sections`).
#[derive(Default)]
pub struct Sections<'a> {
    /// The open session when no group shows it (archived with Archived shut, or past Recent's
    /// cap): unheaded, first.
    pub current: Vec<Entry<'a>>,
    pub needs: Vec<Entry<'a>>,
    /// The pinned sessions in the pins' order, whatever their state, then the settled ones
    /// newest first: an unread result keeps its place, so reading it doesn't move it.
    pub recent: Vec<Entry<'a>>,
    /// How many of `recent` are pinned: they lead it, and Show more doesn't count them.
    pub pinned: usize,
    /// Recent has more settled sessions than `RECENT_LIMIT`.
    pub more: bool,
    /// On the shelf below the list, or a group between Recent and Archived (`workingPlacement`).
    pub working: Vec<Entry<'a>>,
    /// Index in `working` of the currently selected session, kept visible while collapsed (`opencode#66500`).
    pub selected_working: Option<usize>,
    /// Archived sessions, the last archived first: listed under a header that folds.
    pub archived: Vec<Entry<'a>>,
}

impl<'a> Sections<'a> {
    /// The rows a person can reach, in the order they show under `ui`: Working after Archived
    /// while it's the shelf, and collapsed Working still includes its selected row (`opencode#66500`).
    pub fn rows(&self, ui: crate::restore::InboxUi) -> Vec<&Entry<'a>> {
        let working: &[Entry<'a>] = if ui.working_expanded {
            &self.working
        } else if let Some(at) = self.selected_working.filter(|&i| i < self.working.len()) {
            &self.working[at..=at]
        } else {
            &[]
        };
        let archived: &[Entry<'a>] = if ui.archived_expanded { &self.archived } else { &[] };
        let list = self.current.iter().chain(&self.needs).chain(&self.recent);
        match ui.working_placement {
            crate::restore::WorkingPlacement::Shelf => list.chain(archived).chain(working).collect(),
            crate::restore::WorkingPlacement::Inline => list.chain(working).chain(archived).collect(),
        }
    }
}

/// When the session's current run started (`workingSince`): none while the run it holds ended
/// (`time.idle` after `time.busy`) or the server never recorded one.
pub fn working_since(s: &SessionInfo) -> Option<i64> {
    let busy = s.busy?;
    if s.idle.is_some_and(|idle| idle > busy) {
        return None;
    }
    Some(busy)
}

impl App {
    /// The inbox's rows across every listed server, newest first (`createSessionInbox().entries`).
    /// A server that refused our credentials lists nothing until it signs in again.
    pub fn inbox_entries(&self) -> Vec<Entry<'_>> {
        let here = crate::servers::runtime::active(self);
        let mut list: Vec<Entry<'_>> = self.sessions.iter().filter(|s| s.parent.is_none()).map(|s| self.inbox_entry(here, s)).collect();
        // The session list leaves archived sessions out: the newest of them, read on their own.
        list.extend(self.inbox_archived().iter().filter(|a| self.session(&a.id).is_none()).map(|a| self.inbox_entry(here, a)));
        let listed = crate::servers::runtime::listed();
        let shown = |key: &ServerKey| key != here && listed.iter().any(|l| l.key == *key && !l.signed_out);
        list.extend(self.sidebar_inbox.servers.iter().filter(|(key, _)| shown(key)).flat_map(|(server, rows)| {
            rows.iter().map(move |r| Entry { server, session: &r.session, active_ms: r.session.updated, status: r.status, since: r.since, project: r.project.as_ref(), worktree: r.worktree, branch: r.branch.as_deref() })
        }));
        // Rows whose times tie stand in the servers' listed order, whichever server the window
        // works against (`inbox-model.ts:164-166`, `:228`: each connection's sessions in
        // `servers.visible`'s order, then a stable sort by time): a step from one server's row to
        // another's doesn't reorder the rows under it.
        let place = |e: &Entry<'_>| listed.iter().position(|l| l.key == *e.server).unwrap_or(usize::MAX);
        list.sort_by_key(|e| (std::cmp::Reverse(e.active_ms), place(e)));
        list
    }

    /// The row `shown` names as it stands now (`inbox_order`): one lookup, not the whole list.
    pub fn inbox_entry_of<'a>(&'a self, shown: &crate::inbox_order::Shown) -> Option<Entry<'a>> {
        let here = crate::servers::runtime::active(self);
        if shown.server == *here {
            return self.session(&shown.id).or_else(|| self.inbox_archived().iter().find(|a| a.id == shown.id)).map(|s| self.inbox_entry(here, s));
        }
        let (server, rows) = self.sidebar_inbox.servers.iter().find(|(key, _)| *key == shown.server)?;
        let r = rows.iter().find(|r| r.session.id == shown.id)?;
        Some(Entry { server, session: &r.session, active_ms: r.session.updated, status: r.status, since: r.since, project: r.project.as_ref(), worktree: r.worktree, branch: r.branch.as_deref() })
    }

    fn inbox_entry<'a>(&'a self, server: &'a ServerKey, session: &'a SessionInfo) -> Entry<'a> {
        let (status, since) = self.inbox_status(session);
        let project = self.project_for_session(session).or_else(|| self.project_at(&session.directory));
        Entry { server, session, active_ms: self.recent_page.active_ms(session), status, since, project, worktree: in_worktree(self, session), branch: self.sidebar_inbox.branches.get(server, &session.directory) }
    }

    /// A root session's state and when its age counts from, on the server the window works
    /// against.
    fn inbox_status(&self, session: &SessionInfo) -> (InboxStatus, Option<i64>) {
        let root = session.id.as_str();
        let (permission, form) = self.family_requests(root);
        let busy = self.family_busy(root);
        let status = if permission.is_some() {
            InboxStatus::Approval
        } else if form.is_some() {
            InboxStatus::Input
        } else if busy {
            InboxStatus::Working
        } else if !session.unread() && !crate::session_menu::unread::manual(self, crate::servers::runtime::active(self), root) {
            InboxStatus::Ready
        } else if session.outcome.as_deref() == Some("failed") {
            InboxStatus::Failed
        } else {
            InboxStatus::Done
        };
        // A waiting session ages from the request it shows, not from one auto-accept answers.
        let since = match status {
            InboxStatus::Working => working_since(session),
            InboxStatus::Approval | InboxStatus::Input => {
                let request = if status == InboxStatus::Approval { permission } else { form };
                request
                    .and_then(|id| self.sidebar_inbox.asked.get(id).copied())
                    .or_else(|| working_since(session))
                    .or(Some(session.updated))
            }
            _ => session.idle.or(Some(session.updated)),
        };
        (status, since)
    }

    /// Whether `e` is the open session, or the root of the open subagent, on its server.
    pub fn inbox_selected(&self, e: &Entry<'_>) -> bool {
        matches!(&*self.route, Route::Session { id, server } if !self.settings_open() && server == e.server && self.root_of(id) == e.session.id)
    }

    /// The newest archived root sessions of the server the window works against.
    pub(crate) fn inbox_archived(&self) -> &[SessionInfo] {
        self.sidebar_inbox.archives.get(crate::servers::runtime::active(self)).map_or(&[], |a| a.list.as_slice())
    }

    /// The groups the sidebar shows (`SidebarInbox`'s `sections`).
    pub fn inbox_sections(&self) -> Sections<'_> {
        let ui = self.kernel.slots.get(&crate::restore::RESTORE).inbox;
        self.inbox_sections_with(ui.recent_expanded, ui.archived_expanded)
    }

    /// [`App::inbox_sections`] with Recent and Archived shut or open (`expanded`, `archived`)
    /// whatever the window's toggles say.
    pub fn inbox_sections_with(&self, expanded: bool, archived: bool) -> Sections<'_> {
        let list = self.inbox_entries();
        // Pins are shared with the other sidebar layouts. They lead Recent in their own order,
        // whatever the session's state (`inbox.tsx` `sections`).
        let pins = &crate::tabs::pins::state(self).doc;
        let rank = |e: &Entry<'_>| pins.refs().position(|r| r.server == *e.server && r.session_id == e.session.id);
        let mut top: Vec<(usize, Entry<'_>)> = list.iter().filter_map(|e| rank(e).map(|at| (at, *e))).collect();
        top.sort_by_key(|(at, _)| *at);
        // An automation's sessions (Flow's) stay out of it; the open one still shows, as `current`.
        let rest: Vec<Entry<'_>> = list.iter().filter(|e| rank(e).is_none() && !crate::view::is_automation(&e.session.title)).copied().collect();
        let active = || rest.iter().filter(|e| e.session.archived.is_none()).copied();
        // An archived session that asks for something joins Needs attention until it's answered,
        // then goes back to Archived (`inbox.tsx` `sections`, opencode#66327).
        let mut needs: Vec<_> = rest.iter().filter(|e| e.waiting()).copied().collect();
        needs.sort_by_key(|e| e.since.unwrap_or(i64::MAX));
        let mut settled: Vec<_> = active().filter(|e| matches!(e.status, InboxStatus::Done | InboxStatus::Failed | InboxStatus::Ready)).collect();
        settled.sort_by_key(|e| std::cmp::Reverse(e.since.unwrap_or(0)));
        let more = settled.len() > RECENT_LIMIT;
        if !expanded {
            settled.truncate(RECENT_LIMIT);
        }
        let pinned = top.len();
        let recent: Vec<_> = top.into_iter().map(|(_, e)| e).chain(settled).collect();
        // `time.updated` moves with every message of a run, so Working keeps run-start order.
        let mut working: Vec<_> = active().filter(|e| e.status == InboxStatus::Working).collect();
        working.sort_by(|a, b| {
            let start = |e: &Entry<'_>| working_since(e.session).unwrap_or(i64::MAX);
            start(a).cmp(&start(b)).then_with(|| a.session.id.cmp(&b.session.id))
        });
        let mut filed: Vec<_> = rest.iter().filter(|e| e.session.archived.is_some() && !e.waiting()).copied().collect();
        filed.sort_by_key(|e| std::cmp::Reverse(e.session.archived.unwrap_or(0)));
        let listed: &[Entry<'_>] = if archived { &filed } else { &[] };
        let shown = |e: &Entry<'_>| [&needs[..], &recent[..], &working[..], listed].iter().any(|g| g.iter().any(|x| x.server == e.server && x.session.id == e.session.id));
        let selected_working = working.iter().position(|e| self.inbox_selected(e));
        let current = list.iter().find(|e| self.inbox_selected(e)).filter(|e| !shown(e)).copied().into_iter().collect();
        Sections { current, needs, recent, pinned, more, working, selected_working, archived: filed }
    }

    /// The Working shelf opens or shuts (`workingExpanded`).
    pub fn toggle_inbox_working(&mut self) { let w = &mut self.kernel.slots.get_mut(&crate::restore::RESTORE).inbox.working_expanded; *w = !*w; self.record_inbox_shown(); }

    /// Archived opens or shuts (`archivedExpanded`).
    pub fn toggle_inbox_archived(&mut self) { let a = &mut self.kernel.slots.get_mut(&crate::restore::RESTORE).inbox.archived_expanded; *a = !*a; self.record_inbox_shown(); }

    /// Working sessions go to the shelf or into the list (`workingPlacement`).
    pub fn place_inbox_working(&mut self, at: crate::restore::WorkingPlacement) { self.kernel.slots.get_mut(&crate::restore::RESTORE).inbox.working_placement = at; self.record_inbox_shown(); }

    /// Show more / Show less under Recent (`recentExpanded`).
    pub fn toggle_inbox_recent(&mut self) { let r = &mut self.kernel.slots.get_mut(&crate::restore::RESTORE).inbox.recent_expanded; *r = !*r; self.record_inbox_shown(); }

    /// The server and session the inbox row `key` names (`Entry::key`): a session of the server
    /// the window works against when no row of another server has that key.
    pub fn inbox_target(&self, key: &str) -> (ServerKey, String) {
        let here = crate::servers::runtime::active(self);
        if let Some(info) = self.sessions.iter().chain(self.inbox_archived()).find(|s| crate::tabs::session_uid(here, &s.id) == key) {
            return (here.clone(), info.id.clone());
        }
        let found = self.sidebar_inbox.servers.iter().find_map(|(server, rows)| rows.iter().find(|r| crate::tabs::session_uid(server, &r.session.id) == key).map(|r| (server.clone(), r.session.id.clone())));
        found.unwrap_or_else(|| (here.clone(), key.to_owned()))
    }

    /// An inbox row opens its session, as a tab (`inbox.open`), on its server.
    pub fn open_inbox_session(&mut self, key: &str, tx: &mut Tx) {
        let (server, id) = self.inbox_target(key);
        let (title, dir) = self.sidebar_inbox.servers.iter().find(|(k, _)| *k == server).and_then(|(_, rows)| rows.iter().find(|r| r.session.id == id)).map(|r| (r.session.title.clone(), r.session.directory.clone())).unwrap_or_default();
        crate::servers::runtime::switch(self, &server);
        if !self.tabs.iter().any(|t| t.is(&server, &id) && !t.is_draft()) {
            let (title, dir) = self.session(&id).or_else(|| self.inbox_archived().iter().find(|a| a.id == id)).map_or((title, dir), |s| (s.title.clone(), s.directory.clone()));
            self.tabs.push(crate::tabs::Tab::session(id.clone(), title, dir).on(server));
            self.save_tabs();
        }
        self.open(&id, tx);
        self.record_inbox_shown();
    }

    /// Runs `f` on the server of inbox row `key`, with its session's id.
    pub fn on_inbox_row(&mut self, key: &str, f: impl FnOnce(&mut App, &str)) {
        let (server, id) = self.inbox_target(key);
        crate::servers::runtime::with_server(self, &server, |s| f(s, &id));
    }

    /// Archive (`inbox.archive`): beyond Desktop, which waits for the server, the session leaves
    /// the list and its tabs at once; a refusal puts it back (`archive_moves`).
    pub fn archive_session(&mut self, id: &str, tx: &mut Tx) {
        let now = *self.now_ms;
        let here = crate::servers::runtime::active(self).clone();
        // Capture the pre-archive inbox row order before marking the session archived (`opencode#66558`).
        self.record_inbox_shown();
        let gone_key = crate::tabs::session_uid(&here, id);
        let archive_current = self.nav_current().as_deref() == Some(gone_key.as_str());
        let Some(info) = self.sessions.iter_mut().find(|s| s.id == id) else { return };
        info.archived = Some(now);
        let info = info.clone();
        self.inbox_file(info);
        let tab = self.tabs.iter().find(|t| t.is(&here, id) && !t.is_draft()).map(crate::tabs::Tab::uid);
        let was = match &tab {
            None => super::moves::TabWas::Absent,
            Some(uid) if self.current_tab().as_deref() == Some(uid.as_str()) => super::moves::TabWas::Shown,
            Some(_) => super::moves::TabWas::Behind,
        };
        let successor = archive_current
            .then(|| self.pick_inbox_successor(&here, &[id.to_owned()]))
            .flatten();
        if let Some(next) = successor {
            // Open the successor before closing the archived tab (`opencode#66558`).
            self.open_inbox_session(&next, tx);
        }
        if let Some(uid) = &tab {
            self.close_tab(uid, tx);
        } else {
            let routed_family = matches!(&*self.route, Route::Session { id: rid, .. }
                if rid == id
                    || self.ancestors(rid).any(|a| a == id)
                    || self.ancestors(id).any(|a| a == rid.as_str())
            );
            if !routed_family {
                self.hibernate_browser_tabs_for_session(id);
            }
        }
        let sent = crate::servers::runtime::with_server(self, &here, |s| {
            let sent = s.write(opencode_kit::client::Write::Archive(id.to_owned()));
            s.archive_move_sent(id, super::moves::Was::Listed { tab: was }, sent, tx);
            sent
        })
        .unwrap_or(false);
        if sent {
            // Beyond Desktop: the toast offers it back, tab and all.
            crate::undo::offer(self, opencode_i18n::t!("rune.undo.sessionArchived"), crate::undo::Undoable::Archived { server: here, session: id.to_owned() });
        }
    }

    /// An archived session goes on the archived list, first: the session list drops it at its
    /// next read, and the inbox keeps listing it (`inbox-model.ts` `move`).
    pub(crate) fn inbox_file(&mut self, info: SessionInfo) {
        let here = crate::servers::runtime::active(self).clone();
        let filed = &mut self.sidebar_inbox.archives.entry(here).or_default().list;
        filed.retain(|a| a.id != info.id);
        filed.insert(0, info);
    }

    /// Unarchive (`inbox.unarchive`): beyond Desktop, which waits for the server, it comes back
    /// to its group at once; a refusal files it again (`archive_moves`).
    pub fn unarchive_session(&mut self, id: &str) {
        let was = self.session(id).or_else(|| self.inbox_archived().iter().find(|a| a.id == id)).and_then(|i| i.archived);
        self.unarchive_session_locally(id);
        let sent = self.write(opencode_kit::client::Write::Unarchive(id.to_owned()));
        if let Some(at) = was {
            // Putting an archived session back opens no tab: nothing here acts on the window.
            self.archive_move_sent(id, super::moves::Was::Archived(at), sent, &mut Tx::default());
        }
    }

    /// Shows the session as restored: off the archived list, and among the sessions, not
    /// archived (`inbox-model.ts` `move`).
    pub(crate) fn unarchive_session_locally(&mut self, id: &str) {
        let here = crate::servers::runtime::active(self).clone();
        let filed = self.sidebar_inbox.archives.get_mut(&here).and_then(|a| a.list.iter().position(|a| a.id == id).map(|at| a.list.remove(at)));
        match self.sessions.iter_mut().find(|s| s.id == id) {
            Some(info) => info.archived = None,
            None => self.sessions.extend(filed.map(|info| SessionInfo { archived: None, ..info })),
        }
    }
}

/// The inbox's own state: when requests were asked, the list's height for the shelf's cap, and
/// the rows' branches.
#[derive(Default)]
pub struct SidebarInbox {
    /// Wall-clock ms each open request arrived, by request id: a snapshot's requests have no
    /// time, live ones age from their event.
    pub asked: HashMap<String, i64>,
    /// The inbox's laid-out height: the Working shelf takes at most 40% of it.
    pub height: f32,
    /// The age labels' last tick: seconds while something works, minutes otherwise.
    tick: i64,
    /// Each server's newest archived root sessions (its session list leaves them out), read
    /// while the window works against it.
    pub(crate) archives: HashMap<ServerKey, Archive>,
    /// The archives and unarchives sent and not answered yet.
    pub(crate) moves: super::moves::Moves,
    pub branches: Branches,
    /// The root sessions of each server the window isn't checked out on, in listing order, as of
    /// its last checkout (`snap`).
    pub(super) servers: Vec<(ServerKey, Vec<Row>)>,
    /// Another server's rows changed since the inbox last drew.
    pub(crate) redraw: bool,
    /// The first load, which shows Desktop's loader in the inbox's place.
    settle: Settle,
    /// The rows in order, as last read (`inbox_order`), and the reading whose folders went
    /// to the branch reads.
    pub(crate) order: crate::inbox_order::Kept,
    enqueued: u64,
    /// A row scrolled to, focused once the scroll has built it (`view::inbox`).
    pub(crate) focus_next: Option<(String, bool)>,
    /// Last visible inbox order for the open non-archived session (`inbox.tsx:214`, `opencode#66558`).
    pub(crate) shown: Option<(String, Vec<String>)>,
    /// Focus-rescue state machine (`focus-rescue.tsx`, `opencode#65690`, `opencode#66547`).
    pub(crate) focus: super::focus::FocusRescue,
}

/// The longest the inbox hides behind its loader on first open, so a stalled server can't hide
/// it for good (`INBOX_SETTLE_TIMEOUT_MS`).
const SETTLE_TIMEOUT_MS: i64 = 10_000;

/// Whether the reveal timer (`SETTLE_TIMEOUT_MS`) has started (`timerArmed` in `inbox-settle-machine.ts`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum RevealTimer { #[default] Unarmed, Armed { since: i64 } }

/// The inbox's first load (`packages/app/src/shell/titlebar/inbox-settle-machine.ts`): settles
/// once the session list is in and every working root's requests are read, or when the timeout
/// passes; once settled it stays settled, so a session starting later never blanks it. Unlike
/// Desktop, `session_status::RequestReads` owns pending-request fetches, so `heard` lives only
/// in `Loading` and drops on settle (and there is no `Disposed` variant).
#[derive(Clone, Debug)]
enum Settle { Loading { timer: RevealTimer, heard: Heard }, Settled }
impl Default for Settle {
    fn default() -> Self { Self::Loading { timer: RevealTimer::Unarmed, heard: Heard::default() } }
}

enum SettleEvent<'a> { HeardSessions, HeardPending(&'a str), Sync { live: bool, working: &'a [String], now: i64 } }
enum SettleEffect { None, PendingAdded, WakeAfter(Duration), Settled }

impl Settle {
    fn step(&mut self, event: SettleEvent<'_>) -> SettleEffect {
        match (self, event) {
            (Self::Loading { heard, .. }, SettleEvent::HeardSessions) => { heard.sessions = true; SettleEffect::None }
            (Self::Loading { heard, .. }, SettleEvent::HeardPending(s)) => {
                if heard.read.insert(s.to_owned()) { SettleEffect::PendingAdded } else { SettleEffect::None }
            }
            (state @ Self::Loading { .. }, SettleEvent::Sync { live, working, now }) => {
                let Self::Loading { timer, heard } = state else { unreachable!() };
                let since = match *timer {
                    RevealTimer::Unarmed if !live => { *state = Self::Settled; return SettleEffect::Settled; }
                    RevealTimer::Unarmed => { *timer = RevealTimer::Armed { since: now }; now }
                    RevealTimer::Armed { since } => since,
                };
                if !(heard.sessions && working.iter().all(|id| heard.read.contains(id))) && now - since < SETTLE_TIMEOUT_MS {
                    return SettleEffect::WakeAfter(Duration::from_millis((since + SETTLE_TIMEOUT_MS - now).max(1) as u64));
                }
                *state = Self::Settled;
                SettleEffect::Settled
            }
            (Self::Settled, _) => SettleEffect::None,
        }
    }
}

/// What the first load waits on: the session list, and which sessions' pending requests are read.
#[derive(Clone, Debug, Default)]
struct Heard { sessions: bool, read: HashSet<String> }

type ArchivedSlot = Arc<Mutex<Option<Result<Vec<SessionInfo>, ()>>>>;

/// A server's archived root sessions as last read, and the read.
#[derive(Default)]
pub(crate) struct Archive {
    pub(crate) list: Vec<SessionInfo>,
    fetch: ArchivedFetch,
}

/// One-shot fetch of the newest archived root sessions (`ARCHIVED_LIMIT`): mirrors [`Branches`]'s
/// bounded backoff retries (`BRANCH_RETRY_LIMIT`) on failure before resting as `Failed`.
#[derive(Default)]
enum ArchivedFetch { #[default] Idle, Loading { failures: u32, landed: ArchivedSlot }, Retrying { failures: u32, due: f64 }, Loaded, Failed }
enum ArchivedEvent { Poll { now: f64 }, Landed { result: Result<Vec<SessionInfo>, ()>, now: f64 }, SpawnFailed }
enum ArchivedEffect { Spawn(ArchivedSlot), WakeAfter(Duration), Loaded(Vec<SessionInfo>) }

impl ArchivedFetch {
    fn begin(&mut self, failures: u32) -> Option<ArchivedEffect> {
        let slot: ArchivedSlot = Arc::new(Mutex::new(None));
        *self = Self::Loading { failures, landed: Arc::clone(&slot) };
        Some(ArchivedEffect::Spawn(slot))
    }

    fn step(&mut self, event: ArchivedEvent) -> Option<ArchivedEffect> {
        match (std::mem::take(self), event) {
            (Self::Idle, ArchivedEvent::Poll { .. }) => self.begin(0),
            (Self::Retrying { failures, due }, ArchivedEvent::Poll { now }) if due <= now + 1e-9 => self.begin(failures),
            (Self::Retrying { failures, due }, ArchivedEvent::Poll { now }) => {
                *self = Self::Retrying { failures, due };
                Some(ArchivedEffect::WakeAfter(Duration::from_secs_f64((due - now).max(0.0))))
            }
            (Self::Loading { failures, .. }, ArchivedEvent::Landed { result, now }) => match result {
                Ok(list) => { *self = Self::Loaded; Some(ArchivedEffect::Loaded(list)) }
                Err(()) if failures >= BRANCH_RETRY_LIMIT => { *self = Self::Failed; None }
                Err(()) => {
                    let delay = (BRANCH_RETRY_DELAY * f64::from(1u32 << failures)).min(BRANCH_RETRY_MAX_DELAY);
                    *self = Self::Retrying { failures: failures + 1, due: now + delay };
                    Some(ArchivedEffect::WakeAfter(Duration::from_secs_f64(delay)))
                }
            },
            (Self::Loading { failures: 0, .. }, ArchivedEvent::SpawnFailed) => { *self = Self::Idle; None }
            (Self::Loading { failures, .. }, ArchivedEvent::SpawnFailed) => { *self = Self::Retrying { failures, due: 0.0 }; None }
            (state, _) => { *self = state; None }
        }
    }
}

/// Branch lookups each run git on the server, so a few at a time (`BRANCH_BATCH`).
pub use super::branches::Branches;
use super::branches::{BRANCH_RETRY_DELAY, BRANCH_RETRY_LIMIT, BRANCH_RETRY_MAX_DELAY};

impl SidebarInbox {
    /// What of the other servers' rows a row's place depends on (`inbox_order`).
    pub(crate) fn other_servers_hash(&self, h: &mut impl std::hash::Hasher) {
        use std::hash::Hash;
        for (server, rows) in &self.servers {
            server.hash(h);
            rows.len().hash(h);
            for r in rows {
                (&r.session.id, &r.session.title, r.session.updated, r.session.archived, r.status, r.since).hash(h);
            }
        }
    }

    /// A request arrived live at `at`.
    pub fn asked(&mut self, id: &str, at: i64) {
        self.asked.entry(id.to_owned()).or_insert(at);
    }

    /// Whether the inbox still shows its loader.
    pub fn loading(&self) -> bool { matches!(self.settle, Settle::Loading { .. }) }

    /// The server's session list arrived.
    pub fn heard_sessions(&mut self) { self.settle.step(SettleEvent::HeardSessions); }

    pub fn settled(&self) -> bool { matches!(self.settle, Settle::Settled) }

    /// A session's pending requests arrived.
    pub fn heard_pending(&mut self, session: &str) -> bool {
        matches!(self.settle.step(SettleEvent::HeardPending(session)), SettleEffect::PendingAdded)
    }

    /// Starts or finishes the first load at `now` (wall ms), waiting on the requests of the
    /// `working` roots: a source with nothing to wait for (a fixture) settles at once. True when
    /// the loader goes.
    fn settle(&mut self, live: bool, working: &[String], now: i64, tx: &mut Tx) -> bool {
        match self.settle.step(SettleEvent::Sync { live, working, now }) {
            SettleEffect::WakeAfter(after) => { tx.wake_after(after); false }
            SettleEffect::Settled => true,
            SettleEffect::None | SettleEffect::PendingAdded => false,
        }
    }
}

/// The inbox's clock: ages tick each second while something works and each minute otherwise,
/// and the shown rows' branches load. True when the sidebar should draw again.
/// The first load's settling, now rather than at the next poll.
pub fn settle_now(s: &mut App, tx: &mut Tx) -> bool {
    if s.sidebar_inbox.settled() { return false; }
    let live = s.live();
    let working = s.inbox_request_ids();
    s.sidebar_inbox.settle(live, &working, *s.now_ms, tx)
}

pub fn poll(s: &mut App, now: f64, tx: &mut Tx) -> bool {
    // Every poll comes here: the inbox is written only when an answered request leaves it.
    if s.sidebar_inbox.asked.keys().any(|id| !s.requests.iter().any(|(_, r)| r == id)) {
        s.sidebar_inbox.asked.retain(|id, _| s.requests.iter().any(|(_, r)| r == id));
    }
    // The new-session page names its folder's branch (`NewSessionBranch`); an explicit draft on
    // screen always loads its branch, while passive Home/Inbox rows stay on `observe`.
    let draft_dir = matches!(*s.route, crate::Route::Draft(_)).then(|| s.draft_dir().to_owned());
    // So does a Home row under the pointer, as Desktop syncs a row's location on hover.
    let hs = s.kernel.slots.get(&crate::home_search::HOME_SEARCH);
    let hovered = (s.route == crate::Route::Home)
        .then(|| hs.hovered.as_deref().and_then(|id| s.session(id).or_else(|| hs.loaded.iter().find(|x| x.id == id))))
        .flatten()
        .map(|i| i.directory.clone());
    // Alt held on Home shows every row's location, so their branches load (`alt-hold.ts`).
    let revealed: Vec<String> = if s.route == crate::Route::Home && hs.alt_held {
        s.home_sessions(None).iter().map(|i| i.directory.clone()).collect()
    } else {
        Vec::new()
    };
    if s.tab_layout() != TabLayout::Inbox {
        if draft_dir.is_none() && hovered.is_none() && revealed.is_empty() {
            return false;
        }
        let server = crate::servers::runtime::active(s).clone();
        s.sidebar_inbox.branches.enqueue_draft(&server, draft_dir);
        s.sidebar_inbox.branches.observe(&server, hovered.into_iter().chain(revealed));
        let waker = (*s.waker).clone();
        return s.sidebar_inbox.branches.poll(&server, &s.source, waker.as_ref(), now, tx);
    }
    // A working root's pending requests load in `session_status::poll`, so a run waiting on a
    // person shows in Needs attention rather than Working (`inbox-settle-machine`).
    // Read first: writing the inbox's state every poll would build the sidebar again each frame.
    if s.sidebar_inbox.focus_next.is_some()
        && let Some((key, ring)) = s.sidebar_inbox.focus_next.take()
    {
        tx.focus(key, ring);
    }
    let mut changed = settle_now(s, tx);
    if s.sidebar_inbox.redraw {
        s.sidebar_inbox.redraw = false;
        changed = true;
    }
    changed |= load_archived(s, now, tx);
    s.record_inbox_shown();
    let order = s.inbox_order();
    let working = order.rows.iter().any(|r| r.working);
    // The rows' folders go to the branch reads once per reading of the rows, not every poll.
    let dirs: Vec<String> = if s.sidebar_inbox.enqueued != order.generation {
        s.sidebar_inbox.enqueued = order.generation;
        order.rows.iter().map(|r| r.directory.clone()).collect()
    } else {
        Vec::new()
    };
    let step = if working { 1000 } else { 60_000 };
    let tick = s.now_ms.div_euclid(step);
    if s.sidebar_inbox.tick != tick {
        s.sidebar_inbox.tick = tick;
        changed = true;
    }
    tx.wake_after(Duration::from_millis((step - s.now_ms.rem_euclid(step)).max(1) as u64));
    let server = crate::servers::runtime::active(s).clone();
    s.sidebar_inbox.branches.enqueue_draft(&server, draft_dir);
    s.sidebar_inbox.branches.observe(&server, dirs.into_iter().chain(hovered).chain(revealed));
    let waker = (*s.waker).clone();
    changed |= s.sidebar_inbox.branches.poll(&server, &s.source, waker.as_ref(), now, tx);
    changed
}

/// Reads the newest archived root sessions once (`ARCHIVED_LIMIT`), retrying on failure with
/// bounded backoff, and takes them in. True when they land.
fn load_archived(s: &mut App, now: f64, tx: &mut Tx) -> bool {
    let here = crate::servers::runtime::active(s).clone();
    // Read first: every poll comes here, and a write to the inbox draws the sidebar again.
    let fetch = s.sidebar_inbox.archives.get(&here).map(|a| &a.fetch);
    let landed = match fetch {
        Some(ArchivedFetch::Loading { landed, .. }) => landed.lock().unwrap_or_else(|e| e.into_inner()).take(),
        _ => None,
    };
    let resting = !matches!(fetch, None | Some(ArchivedFetch::Idle | ArchivedFetch::Retrying { .. }));
    if let Some(result) = landed {
        let archive = s.sidebar_inbox.archives.entry(here).or_default();
        return match archive.fetch.step(ArchivedEvent::Landed { result, now }) {
            Some(ArchivedEffect::Loaded(list)) => {
                // What was archived here while the read ran stays, ahead of what it read.
                let read: Vec<SessionInfo> = list.into_iter().filter(|r| !archive.list.iter().any(|a| a.id == r.id)).collect();
                archive.list.extend(read);
                true
            }
            Some(ArchivedEffect::WakeAfter(after)) => { tx.wake_after(after); false }
            _ => false,
        };
    }
    if resting { return false; }
    let Source::Live { server, .. } = &*s.source else { return false };
    let server = server.clone();
    let wake = (*s.waker).clone();
    let archive = s.sidebar_inbox.archives.entry(here).or_default();
    match archive.fetch.step(ArchivedEvent::Poll { now }) {
        Some(ArchivedEffect::Spawn(slot)) => {
            if rune::task::spawn("inbox-archived", move || {
                let result = server.get("/api/session?parentID=null&archived=true&limit=50").map(|v| opencode_kit::model::parse_sessions(&v)).map_err(|_| ());
                *slot.lock().unwrap_or_else(|e| e.into_inner()) = Some(result);
                if let Some(w) = wake { w(); }
            }).is_err() { archive.fetch.step(ArchivedEvent::SpawnFailed); }
        }
        Some(ArchivedEffect::WakeAfter(after)) => tx.wake_after(after),
        _ => {}
    }
    false
}

/// The runtime is about to leave the server the window is checked out on: while the inbox shows,
/// its root sessions' rows, for while the window works against another server. Their folders'
/// branches load on that server (its own `Branches`, which its half keeps).
pub fn snap(s: &mut App) {
    if s.tab_layout() != TabLayout::Inbox {
        return;
    }
    let key = crate::servers::runtime::active(s).clone();
    let mut rows: Vec<Row> = s
        .sessions
        .iter()
        .filter(|i| i.parent.is_none())
        .chain(s.inbox_archived().iter().filter(|a| s.session(&a.id).is_none()))
        .map(|i| {
            let (status, since) = s.inbox_status(i);
            let project = s.project_for_session(i).or_else(|| s.project_at(&i.directory)).cloned();
            let session = SessionInfo { updated: s.recent_page.active_ms(i), ..i.clone() };
            Row { session, status, since, project, worktree: in_worktree(s, i), branch: None }
        })
        .collect();
    s.sidebar_inbox.branches.observe(&key, rows.iter().map(|r| r.session.directory.clone()));
    for r in &mut rows {
        r.branch = s.sidebar_inbox.branches.get(&key, &r.session.directory).map(str::to_owned);
    }
    let face = |r: &Row| (r.session.id.clone(), r.session.title.clone(), r.session.updated, r.session.archived, r.status, r.since, r.branch.clone(), r.project.as_ref().map(|p| p.id.clone()), r.worktree);
    let inbox = &mut *s.sidebar_inbox;
    match inbox.servers.iter_mut().find(|(k, _)| *k == key) {
        Some((_, list)) => {
            if list.iter().map(face).ne(rows.iter().map(face)) {
                inbox.redraw = true;
            }
            *list = rows;
        }
        None => {
            inbox.redraw = true;
            inbox.servers.push((key, rows));
        }
    }
    let order: Vec<ServerKey> = crate::servers::runtime::listed().into_iter().map(|l| l.key).collect();
    s.sidebar_inbox.servers.retain(|(k, _)| order.contains(k));
    s.sidebar_inbox.servers.sort_by_key(|(k, _)| order.iter().position(|o| o == k));
}

/// `runtime::poll`, for a server the window isn't checked out on: its rows' branches load.
pub fn poll_branches(s: &mut App, now: f64, tx: &mut Tx) -> bool {
    if s.tab_layout() != TabLayout::Inbox {
        return false;
    }
    let server = crate::servers::runtime::active(s).clone();
    let waker = (*s.waker).clone();
    s.sidebar_inbox.branches.poll(&server, &s.source, waker.as_ref(), now, tx)
}

/// The age beside a row: "now" under a minute ("12s" while working), then m, h, d.
pub fn age(status: InboxStatus, since: Option<i64>, now_ms: i64) -> Option<String> {
    let seconds = ((now_ms - since?) / 1000).max(0);
    Some(match seconds {
        s if s < 60 && status == InboxStatus::Working => opencode_i18n::plural!("sidebar.inbox.elapsed.seconds", s),
        s if s < 60 => opencode_i18n::t!("sidebar.inbox.time.now").into(),
        s if s < 3600 => opencode_i18n::plural!("sidebar.inbox.elapsed.minutes", s / 60),
        s if s < 86_400 => opencode_i18n::plural!("sidebar.inbox.elapsed.hours", s / 3600),
        s => opencode_i18n::plural!("sidebar.inbox.elapsed.days", s / 86_400),
    })
}

/// Whether `dir` is a worktree of its project: a folder other than the project's own.
pub fn in_worktree(s: &App, info: &SessionInfo) -> bool {
    let root = s.project_for_session(info).map(|p| p.directory.as_str());
    root.is_some_and(|root| !same_path(root, &info.directory))
}

fn same_path(a: &str, b: &str) -> bool {
    let norm = |p: &str| p.trim_end_matches(['/', '\\']).replace('\\', "/").to_lowercase();
    norm(a) == norm(b)
}

/// The layout's test scene: Home with the fixture's sessions in the Inbox, one in each state. The
/// fixture already has a permission waiting, two runs and two unread; this adds a question
/// waiting, a failed run and when the runs started.
fn scene(s: &mut App) {
    *s.layout = crate::layout_state::Shell::at_rest(TabLayout::Inbox);
    s.settings.tab_layout = TabLayout::Inbox;
    let now = *s.now_ms;
    s.requests.insert(("ses_fx0003aaaaaaaaaaaaaaaaaa".into(), "que_fx0001".into()));
    for (id, busy) in [("ses_fx0006aaaaaaaaaaaaaaaaaa", 95_000), ("ses_fx0018aaaaaaaaaaaaaaaaaa", 12_000)] {
        if let Some(info) = s.sessions.iter_mut().find(|i| i.id == id) {
            info.busy = Some(now - busy);
        }
    }
    if let Some(info) = s.sessions.iter_mut().find(|i| i.id == "ses_fx0007aaaaaaaaaaaaaaaaaa") {
        info.outcome = Some("failed".into());
    }
}

rune_register::register!(crate::scenes::SCENE_LIST: crate::scenes::Scene = [("layout-inbox", scene)]);
