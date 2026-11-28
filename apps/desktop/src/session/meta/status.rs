//! What every list shows for a session, from the one session store (`App::running`, `inboxes`,
//! `requests` and `pending`, and the sessions' read marks): Desktop's `sessionRequests`,
//! `sessionBusy` and `useSessionTabAvatarState` (`shell/layout/project-avatar-state.ts`). Tabs,
//! the palette, Home's rows and the Inbox all read it here.
//!
//! Pending permissions and forms load as Desktop's data store syncs them
//! (`session.permission.sync`, `session.form.sync`): each session's once per connection, for the
//! open session and what runs below it (`session/requests/model.ts`), an inactive tab whose family
//! runs (`tab-strip.tsx`) and a root that works, for the Inbox (`inbox-settle-machine.ts`) and
//! for Home's rows, so a row waiting on a person shows its unread dot as Desktop's does. The
//! `permission.*` and `form.*` events keep them current (`session::events`).

use std::collections::HashMap;
use std::time::Duration;

use opencode_kit::client::Request;
use opencode_kit::model::{Pending, SessionInfo};
use rune::Tx;

use crate::layout_state::TabLayout;
use crate::{App, Route, Source};

/// A tab's requests load this long after it shows, the next tab's 50 ms later (`tab-strip.tsx`).
const TAB_DELAY: f64 = 0.3;
const TAB_STEP: f64 = 0.05;

/// A session's marks wherever it's listed (`useSessionTabAvatarState` for a root).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Marks {
    /// A permission or a form waits on a person somewhere in the family.
    pub attention: bool,
    /// Something in the family works, and nothing waits on a person.
    pub busy: bool,
    /// Waiting on a person, or finished since it was last read.
    pub unread: bool,
}

/// Retry backoff for a failed `/permission` or `/form` GET: 1 s doubling up to 30 s.
const RETRY_BASE: f64 = 1.0;
const RETRY_MAX: f64 = 30.0;

/// Per-session pending-request fetch state on the current connection (absence means unread).
#[derive(Clone, Copy, Debug, PartialEq)]
enum SessionRead {
    /// Inactive busy tab waiting out its stagger delay until `at` (`App::clock`).
    Scheduled { at: f64 },
    /// A `Request::Pending` fetch stamped with `epoch` is currently in flight.
    InFlight { epoch: u64, attempts: u32 },
    /// A prior fetch failed transiently; retry at `at` (`App::clock`).
    Retry { at: f64, attempts: u32 },
    /// Both permission and form reads succeeded on this connection.
    Read,
}

enum ReadEvent {
    /// Request a session's pending permissions and forms immediately.
    ReadNow(String),
    /// Reconcile inactive busy tabs and advance scheduled/retry timers at `now`.
    SyncTabs { tabs: Vec<(usize, String)>, now: f64 },
    /// A `Request::Pending` reply arrived for `session` at `epoch`.
    Settled { session: String, epoch: u64, failed: bool, now: f64 },
    /// Connection dropped: forget all read marks and pending schedules.
    Reset,
}

enum ReadEffect {
    Fetch { session: String, epoch: u64 },
    WakeAfter(Duration),
}

/// Which sessions' pending requests this connection has read (`sync.run`'s done keys), the
/// tabs waiting out their delay or retry backoff, and SSE arrival epochs so a delayed GET
/// snapshot never deletes a request that arrived over SSE while the GET was in flight.
#[derive(Default)]
pub struct RequestReads {
    sessions: HashMap<String, SessionRead>,
    /// Strictly monotonic across reconnects: bumped on every fetch dispatch, SSE request arrival,
    /// and connection reset.
    epoch: u64,
    /// `epoch` at the most recent `Reset`; any `Incoming::Pending` with `epoch <= reset_epoch`
    /// predates the reconnect and is ignored.
    reset_epoch: u64,
    /// `request_id -> epoch` when `permission.asked` / `form.created` arrived over SSE.
    sse_seen: HashMap<String, u64>,
}

impl RequestReads {
    fn next_epoch(&mut self) -> u64 {
        self.epoch += 1;
        self.epoch
    }

    fn is_read_or_active(&self, id: &str) -> bool {
        matches!(self.sessions.get(id), Some(SessionRead::Read | SessionRead::InFlight { .. } | SessionRead::Retry { .. }))
    }

    fn has_timers(&self) -> bool {
        self.sessions.values().any(|s| matches!(s, SessionRead::Scheduled { .. } | SessionRead::Retry { .. }))
    }

    /// Stamps an SSE `permission.asked` / `form.created` arrival after every already-dispatched
    /// fetch epoch so an in-flight GET cannot prune the live request.
    pub(crate) fn note_sse(&mut self, request_id: &str) {
        let ep = self.next_epoch();
        self.sse_seen.insert(request_id.to_owned(), ep);
    }

    pub(crate) fn forget_sse(&mut self, request_id: &str) {
        self.sse_seen.remove(request_id);
    }

    /// True if `request_id` arrived via SSE strictly after `fetch_epoch` was dispatched.
    pub(crate) fn arrived_after(&self, request_id: &str, fetch_epoch: u64) -> bool {
        self.sse_seen.get(request_id).is_some_and(|&ep| ep > fetch_epoch)
    }

    /// True if `Incoming::Pending { session, epoch, .. }` predates a reconnect or a newer fetch.
    pub(crate) fn is_stale(&self, session: &str, epoch: u64) -> bool {
        if epoch <= self.reset_epoch {
            return true;
        }
        matches!(self.sessions.get(session), Some(SessionRead::InFlight { epoch: cur, .. }) if *cur > epoch)
    }

    fn step(&mut self, event: ReadEvent) -> Vec<ReadEffect> {
        match event {
            ReadEvent::ReadNow(id) => {
                let attempts = match self.sessions.get(&id) {
                    Some(SessionRead::Read | SessionRead::InFlight { .. }) => return Vec::new(),
                    Some(SessionRead::Retry { attempts, .. }) => *attempts,
                    Some(SessionRead::Scheduled { .. }) | None => 0,
                };
                let epoch = self.next_epoch();
                self.sessions.insert(id.clone(), SessionRead::InFlight { epoch, attempts });
                vec![ReadEffect::Fetch { session: id, epoch }]
            }
            ReadEvent::SyncTabs { tabs, now } => {
                self.sessions.retain(|id, state| {
                    !matches!(state, SessionRead::Scheduled { .. }) || tabs.iter().any(|(_, t)| t == id)
                });
                for (index, id) in tabs {
                    self.sessions
                        .entry(id)
                        .or_insert(SessionRead::Scheduled { at: now + TAB_DELAY + TAB_STEP * index as f64 });
                }
                let mut effects = Vec::new();
                let mut next: Option<f64> = None;
                let mut due: Vec<(String, u32)> = Vec::new();
                for (id, state) in &self.sessions {
                    match *state {
                        SessionRead::Scheduled { at } if at <= now + 1e-9 => due.push((id.clone(), 0)),
                        SessionRead::Retry { at, attempts } if at <= now + 1e-9 => due.push((id.clone(), attempts)),
                        SessionRead::Scheduled { at } | SessionRead::Retry { at, .. } => {
                            next = Some(next.map_or(at, |m| m.min(at)));
                        }
                        SessionRead::InFlight { .. } | SessionRead::Read => {}
                    }
                }
                for (id, attempts) in due {
                    let epoch = self.next_epoch();
                    self.sessions.insert(id.clone(), SessionRead::InFlight { epoch, attempts });
                    effects.push(ReadEffect::Fetch { session: id, epoch });
                }
                if let Some(at) = next {
                    effects.push(ReadEffect::WakeAfter(Duration::from_secs_f64((at - now).max(0.01))));
                }
                effects
            }
            ReadEvent::Settled { session, epoch, failed, now } => {
                if self.is_stale(&session, epoch) {
                    return Vec::new();
                }
                let attempts = match self.sessions.get(&session) {
                    Some(SessionRead::InFlight { epoch: cur, attempts }) if *cur == epoch => *attempts,
                    Some(SessionRead::InFlight { .. }) => return Vec::new(),
                    _ => 0,
                };
                if !failed {
                    self.sessions.insert(session, SessionRead::Read);
                    return Vec::new();
                }
                let next_attempts = attempts.saturating_add(1);
                let delay = (RETRY_BASE * f64::from(1u32 << next_attempts.saturating_sub(1).min(5))).min(RETRY_MAX);
                self.sessions.insert(session, SessionRead::Retry { at: now + delay, attempts: next_attempts });
                vec![ReadEffect::WakeAfter(Duration::from_secs_f64(delay))]
            }
            ReadEvent::Reset => {
                self.sessions.clear();
                self.reset_epoch = self.next_epoch();
                Vec::new()
            }
        }
    }
}

fn apply_effects(s: &mut App, effects: Vec<ReadEffect>, mut tx: Option<&mut Tx>) {
    for effect in effects {
        match effect {
            ReadEffect::Fetch { session, epoch } => {
                if let Source::Live { requests: Some(r), .. } = &*s.source {
                    r.send(Request::Pending { session, epoch }).ok();
                }
            }
            ReadEffect::WakeAfter(delay) => {
                if let Some(tx) = tx.as_deref_mut() {
                    tx.wake_after(delay);
                }
            }
        }
    }
}

impl App {
    /// `id`'s marks, across its family.
    pub fn marks(&self, id: &str) -> Marks {
        let (permission, form) = self.family_requests(id);
        let attention = permission.is_some() || form.is_some();
        Marks {
            attention,
            busy: !attention && self.family_busy(id),
            unread: attention || self.session(id).is_some_and(SessionInfo::unread) || crate::session_menu::unread::manual(self, crate::servers::runtime::active(self), id),
        }
    }

    /// Whether anything in `root`'s family runs, has a prompt or compaction waiting in its inbox,
    /// or holds a live permission request waiting on approval (`sessionBusy`).
    pub fn family_busy(&self, root: &str) -> bool {
        self.has_live_family(root)
            || self.inboxes.iter().any(|(id, inbox)| (!inbox.items.is_empty() || inbox.compaction.is_some()) && self.root_of(id) == root)
            || self.pending.iter().any(|p| matches!(p, Pending::Permission { .. }) && self.root_of(p.session()) == root)
    }

    /// Current server activity, excluding historical busy times and queued but inactive work.
    pub(crate) fn has_live_family(&self, root: &str) -> bool {
        self.running.iter().any(|id| self.root_of(id) == root)
    }

    /// Whether any direct child session of `session_id` is currently running (`hasRunningChild` in
    /// `packages/app/src/shell/notifications/notification.tsx:33-43`): only direct children count
    /// (a grandchild reports to its own parent, and running shells do not count).
    pub(crate) fn has_running_child(&self, session_id: &str) -> bool {
        self.running
            .iter()
            .any(|c| c != session_id && self.session(c).and_then(|s| s.parent.as_deref()) == Some(session_id))
    }

    /// The permission and the form waiting on a person anywhere in `root`'s family, by request
    /// id (`sessionPermissionRequest`, `sessionFormRequest`). A permission with a reply on its way,
    /// or one auto-accept answers, doesn't wait (`needsUser`).
    pub fn family_requests(&self, root: &str) -> (Option<&str>, Option<&str>) {
        let mut found: (Option<&str>, Option<&str>) = (None, None);
        let asked = |id: &str| self.sidebar_inbox.asked.get(id).copied().unwrap_or(i64::MAX);
        for (session, id) in &self.requests {
            if self.root_of(session) != root {
                continue;
            }
            let form = match self.pending.iter().find(|p| p.id() == id) {
                Some(Pending::Form { .. }) => true,
                Some(Pending::Permission { .. }) => false,
                None => !id.starts_with("per"),
            };
            if !form && (self.kernel.slots.get(&crate::permission::PERMISSION_SENT).busy(id) || self.auto_accepts(session)) {
                continue;
            }
            let slot = if form { &mut found.1 } else { &mut found.0 };
            // The oldest, so a row's age doesn't depend on the set's order.
            if slot.is_none_or(|have| (asked(id), id.as_str()) < (asked(have), have)) {
                *slot = Some(id.as_str());
            }
        }
        found
    }

    /// Every session id that may currently hold a pending permission or form: running sessions
    /// plus sessions with cached permissions/forms (so reconnect clears requests answered while
    /// offline without booting cold idle subagent locations; `UP-115` `#65120`).
    fn request_candidates(&self) -> impl Iterator<Item = &str> {
        self.running
            .iter()
            .map(String::as_str)
            .chain(self.requests.iter().map(|(s, _)| s.as_str()))
            .chain(self.pending.iter().map(Pending::session))
    }

    /// Unique session ids in `ancestor`'s subtree that may hold a permission or form (plus
    /// `ancestor` itself when `include_ancestor` is true).
    pub(crate) fn family_request_ids(&self, ancestor: &str, include_ancestor: bool) -> Vec<String> {
        let mut ids: Vec<String> = self
            .request_candidates()
            .filter(|sid| *sid != ancestor && self.descends(sid, ancestor))
            .map(str::to_owned)
            .collect();
        if include_ancestor || self.request_candidates().any(|sid| sid == ancestor) {
            ids.push(ancestor.to_owned());
        }
        ids.sort();
        ids.dedup();
        ids
    }

    /// Session ids whose pending requests the Inbox reads and waits on before settling: each
    /// working or request-holding root plus every running or request-holding descendant
    /// (`inbox-model.ts:278-287`, `UP-115`).
    pub(crate) fn inbox_request_ids(&self) -> Vec<String> {
        let mut ids = Vec::new();
        for root in self.sessions.iter().filter(|i| i.parent.is_none()) {
            let live = self.has_live_family(&root.id);
            let mut family: Vec<String> = self
                .request_candidates()
                .filter(|sid| self.root_of(sid) == root.id)
                .map(str::to_owned)
                .collect();
            if live || !family.is_empty() {
                ids.push(root.id.clone());
                ids.append(&mut family);
            }
        }
        ids.sort();
        ids.dedup();
        ids
    }

    /// Reads `id`'s open permissions and forms, once per connection (retrying transient errors).
    pub(crate) fn read_requests(&mut self, id: &str) {
        if !matches!(&*self.source, Source::Live { requests: Some(_), .. }) || self.request_reads.is_read_or_active(id) {
            return;
        }
        let effects = self.request_reads.step(ReadEvent::ReadNow(id.to_owned()));
        apply_effects(self, effects, None);
    }

    /// Settles an in-flight `Request::Pending` reply: marks `Read` on success or schedules
    /// exponential-backoff retry on a transient error.
    pub(crate) fn settle_pending(&mut self, session: &str, epoch: u64, failed: bool, tx: &mut Tx) {
        let now = *self.clock;
        let effects = self.request_reads.step(ReadEvent::Settled {
            session: session.to_owned(),
            epoch,
            failed,
            now,
        });
        apply_effects(self, effects, Some(tx));
    }

    /// The open session's whole family (`root_of(id)`, `id`, and every running or request-holding
    /// sibling/descendant) so viewing a child sub-agent session also recovers parent and sibling
    /// permissions/forms on reconnect (`pending_for` displays the full `root_of` family).
    pub(crate) fn read_open_requests(&mut self, id: &str) {
        let root = self.root_of(id).to_owned();
        let mut ids = self.family_request_ids(&root, true);
        if id != root && !ids.iter().any(|s| s == id) {
            ids.push(id.to_owned());
        }
        for sid in ids {
            self.read_requests(&sid);
        }
    }

    /// The connection dropped: every session's requests read again when next shown.
    pub(crate) fn forget_request_reads(&mut self) {
        let _ = self.request_reads.step(ReadEvent::Reset);
    }
}

/// Starts the reads the shown tabs and the Inbox's working roots are due. True on a change.
pub fn poll(s: &mut App, now: f64, tx: &mut Tx) -> bool {
    if !matches!(*s.source, Source::Live { requests: Some(_), .. }) {
        return false;
    }
    // Keep the open session's running and request-holding family synced once `Active` or
    // `Sessions` arrives after open/reconnect (`session/requests/model.ts:65-81`, `UP-115`).
    let active_open = match &*s.route {
        Route::Session { id, server }
            if server == crate::servers::runtime::active(s) && !s.queue.starts.has(id) =>
        {
            Some(id.clone())
        }
        _ => None,
    };
    if let Some(open_id) = active_open.as_deref() {
        s.read_open_requests(open_id);
    }
    let layout = s.tab_layout();
    // The tab strip (top or side tabs): every session in an inactive tab's family that is running
    // or holds cached requests (`tab-strip.tsx:516-528`, `UP-115`).
    let shown = match &*s.route {
        Route::Session { id, server } if server == crate::servers::runtime::active(s) => Some(s.root_of(id).to_owned()),
        _ => None,
    };
    let tabs: Vec<(usize, String)> = if layout == TabLayout::Inbox {
        Vec::new()
    } else {
        s.tabs
            .iter()
            .enumerate()
            .filter(|(_, t)| !t.is_draft() && t.server == *crate::servers::runtime::active(s) && shown.as_deref() != Some(t.id.as_str()))
            .flat_map(|(i, t)| {
                s.family_request_ids(&t.id, false)
                    .into_iter()
                    .filter(|sid| !s.request_reads.is_read_or_active(sid))
                    .map(move |sid| (i, sid))
            })
            .collect()
    };
    if !tabs.is_empty() || s.request_reads.has_timers() {
        let effects = s.request_reads.step(ReadEvent::SyncTabs { tabs, now });
        apply_effects(s, effects, Some(tx));
    }
    // A live run or cached-request family may be waiting on a person. A saved busy timestamp is
    // not a live run: asking for its permissions/forms can boot a cold location just to draw the
    // Inbox (`inbox-model.ts:278-287`, `UP-115`).
    if layout == TabLayout::Inbox || s.settings_open() {
        for id in s.inbox_request_ids() {
            if !s.request_reads.is_read_or_active(&id) {
                s.read_requests(&id);
            }
        }
    }
    false
}
