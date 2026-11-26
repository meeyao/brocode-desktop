//! The outbox: every prompt this app sent that the server hasn't confirmed, one copy for all of a
//! server's windows (`windows::peer`). Each prompt's way is the machine in `machine.rs`; its row
//! in the database is `store.rs`'s. An absent HTTP reply isn't a rejected prompt: check inbox
//! admission and the exact message before replaying a journaled request with its original ID.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::mpsc::Sender;

use opencode_kit::client::{PathAttachment, Prompt, Server, Write};
use opencode_kit::model::{Attachment, Delivery, Inbox, InboxItem, Message, ModelRef, PromptComment, Transcript};
use opencode_kit::wire_error::WireError;
use rune::TextEdit;
use opencode_ui::prompt_history::Entry;

pub mod machine;
pub mod outbox;
pub mod reason;
pub mod receipt;
pub mod row;
pub mod store;
pub mod view;

pub use machine::{AfterCheck, Phase, SLOW_SEND, SlowSend};
pub use opencode_storage::StorageOp;
pub use opencode_runtime::SendResult;
pub use outbox::{
    enqueue_prepared, enqueue_prompt, enqueue_prompt_first, listen_outbox, open_outbox, outbox_action, pump_outbox, release_prepared, remap_first_after, restore_draft, retry_restored,
    show_outbox, steer_waiting_outbox, tick_outbox,
};
pub use view::{footer, footer_version};

use machine::{Effect, Event, Found, Verdict};

const KEEP: usize = 32;

/// Object-safe host boundary for the durable prompt outbox, delivery receipt checks, retry/edit
/// recovery, and send status footers.
///
/// Every transition, poll, outbox action, and receipt check operates on `&dyn SendFailedHost` /
/// `&mut dyn SendFailedHost`. Generic `H: SendFailedHost + 'static` bounds appear strictly on
/// `rune::El` view wrappers in [`view`].
pub trait SendFailedHost: opencode_kernel::AppEnv + opencode_kernel::AppFeedback {
    /// Durable outbox log (tracked read).
    fn unsent(&self) -> &Log;
    /// Durable outbox log without subscribing view regions to writes (used by `tick_outbox`).
    fn unsent_untracked(&self) -> &Log {
        self.unsent()
    }
    /// Mutable durable outbox log.
    fn unsent_mut(&mut self) -> &mut Log;

    /// Per-session server inbox items and waiting notices.
    fn inboxes(&self) -> &HashMap<String, Inbox>;
    /// Mutable per-session inbox map.
    fn inboxes_mut(&mut self) -> &mut HashMap<String, Inbox>;

    /// Loaded session transcripts.
    fn transcripts(&self) -> &HashMap<String, Transcript>;
    /// Mutable session transcripts.
    fn transcripts_mut(&mut self) -> &mut HashMap<String, Transcript>;

    /// Stable window identifier used to adopt legacy per-window outbox rows on open.
    fn window_id(&self) -> &str {
        ""
    }
    /// Window peer token identifying which window's writer sent an in-flight prompt.
    fn peer_token(&self) -> u64 {
        0
    }
    /// Runs a storage-thread outbox operation (defaults to [`opencode_storage`] process runner).
    fn with_storage(&self, op: StorageOp) {
        opencode_storage::with_storage(op);
    }
    /// Active route's draft document key, cleared in the same transaction as the first outbox row.
    fn active_draft_document(&self) -> Option<String> {
        None
    }
    /// Hands off a draft document to an outbox row and returns the settled draft journal revision.
    fn hand_off_draft_doc(&mut self, key: &str) -> u64 {
        opencode_platform::storage::hand_off_doc(key)
    }

    /// Whether the app is running against an offline fixture.
    fn is_fixture(&self) -> bool {
        false
    }
    /// Whether the server link is currently down/reconnecting.
    fn is_reconnect_down(&self) -> bool {
        false
    }
    /// Active live OpenCode server client, if connected to a live server.
    fn live_server(&self) -> Option<Server>;
    /// Whether the live server connection permits sending prompts (`allow_send`).
    fn live_allow_send(&self) -> bool {
        false
    }
    /// Live server write channel sender, if connected and writable.
    fn live_writer(&self) -> Option<Sender<Write>> {
        None
    }
    /// Queues a write directly on the host (used by local non-journaled retry).
    fn write(&mut self, write: Write) -> bool;
    /// Builds the write used to send `prompt` to `session`.
    fn send_write(&self, session: &str, prompt: Prompt) -> Write {
        Write::Send { session: session.to_owned(), prompt }
    }

    /// Whether the window currently shows `session` on the checked-out server (`Route::here`).
    fn is_session_here(&self, session: &str) -> bool;

    /// Active composer text editor.
    fn composer(&self) -> &TextEdit;
    /// Mutable active composer text editor.
    fn composer_mut(&mut self) -> &mut TextEdit;
    /// Composer file attachments currently staged.
    fn composer_files(&self) -> &[Attachment];
    /// Composer path attachments currently staged.
    fn composer_paths(&self) -> &[PathAttachment];
    /// Whether any composer attachments (files, paths, context, uploads) are currently staged.
    fn composer_has_attachments(&self) -> bool;
    /// Whether composer selection context or in-flight uploads are staged (for restored-draft match).
    fn composer_has_context_or_uploads(&self) -> bool {
        false
    }
    /// Composer quote comments currently staged.
    fn composer_quotes(&self) -> Vec<PromptComment> {
        Vec::new()
    }
    /// Whether any quote comments are currently staged in the composer.
    fn composer_has_quotes(&self) -> bool {
        false
    }
    /// Clears staged composer files, paths, and quotes when retrying a restored failed draft.
    fn clear_composer_attachments_and_quotes(&mut self);
    /// Appends a file attachment to the composer.
    fn add_composer_file(&mut self, file: Attachment);
    /// Appends a path attachment to the composer.
    fn add_composer_path(&mut self, path: PathAttachment);
    /// Appends a quote comment to the composer.
    fn push_composer_quote(&mut self, quote: PromptComment) {
        let _ = quote;
    }
    /// Drops any pending local mention echo entries associated with message `id`.
    fn retain_echoes_without(&mut self, id: &str) {
        let _ = id;
    }
    /// Removes a rejected/restored prompt from recall history (`removeHistoryEntry`, `model.ts:306`).
    fn remove_prompt_history(&mut self, entry: &Entry) {
        let _ = entry;
    }
    /// Restores a rejected prompt into a non-active `session`'s parked draft when empty.
    fn restore_inactive_draft(&mut self, session: &str, entry: Entry, files: Vec<Attachment>, paths: Vec<PathAttachment>, quotes: Vec<PromptComment>) {
        let _ = (session, entry, files, paths, quotes);
    }
    /// Currently selected composer `(agent, model)` pair (used by `retry_restored`).
    fn picked_choice(&self) -> (Option<String>, Option<ModelRef>) {
        (None, None)
    }

    /// Notified when prompt `id`'s [`SendResult`] changes (e.g. quote comment editor send waiter).
    fn on_send_result(&mut self, id: &str, result: SendResult) {
        let _ = (id, result);
    }

    /// Whether `session`'s queue state machine currently has a rewrite/mutation in flight.
    /// While true, [`pump_outbox`] waits before sending another prompt for `session` so new
    /// admissions cannot race with an active `rewrite_queue`.
    fn is_queue_busy(&self, session: &str) -> bool {
        let _ = session;
        false
    }

    /// Notified when a queue-first prompt (`Alt+Enter`, `#63109`) is admitted to `session`'s
    /// server inbox so it moves ahead of existing queued prompts (after any still-queued `after`
    /// anchors). `force` is true when `session`'s inbox has not yet been synced from the server.
    fn on_admitted_first(&mut self, session: &str, id: &str, after: Vec<String>, force: bool) {
        let _ = (session, id, after, force);
    }
}

/// A prompt's phase without its data, as the timeline asks about it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum State {
    #[default]
    Sending,
    Unconfirmed,
    Failed,
    Accepted,
    Waiting,
}

/// The composer draft a prompt's words left, cleared in the transaction of its first row so a
/// crash never finds them both in the outbox and back in the composer (a second send).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) enum LeftDraft {
    #[default]
    None,
    /// Handed off from draft document `key` at journal revision `rev`; cleared with the first row.
    Pending { key: String, rev: u64 },
}

/// One prompt in the outbox: what was typed, what went out, and where it is (`phase`).
#[derive(Clone)]
pub struct Sent {
    pub session: String,
    pub id: String,
    pub draft: Entry,
    pub files: Vec<Attachment>,
    pub paths: Vec<PathAttachment>,
    /// Sent while its session was busy: once the server has it, it waits in the inbox.
    pub inbox: bool,
    pub prompt: Option<Prompt>,
    pub phase: Phase,
    /// Kept in the outbox across launches and sent again until the server has it; a fixture's
    /// or story's local send only rolls back.
    pub journaled: bool,
    pub(crate) operation: outbox::Operation,
    /// Its place in the server's outbox, across launches.
    pub(crate) seq: i64,
    /// Whether it has a row: later steps change its columns.
    pub(crate) row: store::Row,
    /// How many times it was handed to a writer (`Phase::Sending`'s `handoff`).
    pub(crate) handoffs: u32,
    /// The composer draft handed off to this outbox row, if still pending first-row commit.
    pub(crate) left_draft: LeftDraft,
}

/// Whether a receipt check is out.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Check {
    #[default]
    Idle,
    /// Asked; `again` when an unconfirmed prompt joined while it was out.
    Checking { again: bool },
}

/// When the outbox may pump again after a failed send.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) enum Retry {
    #[default]
    Ready,
    Backoff { attempts: u32, until: f64 },
}

#[derive(Default)]
struct ReceiptCheck {
    phase: Check,
    receipts: rune::Latest<Vec<(String, Result<receipt::Receipt, WireError>)>>,
}

#[derive(Default)]
pub struct Log {
    pub(crate) store: store::Store,
    pub(crate) retry: Retry,
    pub(crate) sent: Vec<Sent>,
    checks: HashMap<String, ReceiptCheck>,
}

impl Log {
    /// A journaled prompt is still on its way: the outbox keeps pumping.
    pub fn waiting_for_link(&self) -> bool {
        self.sent.iter().any(|s| s.journaled && matches!(s.phase, Phase::Sending { .. } | Phase::Waiting { .. } | Phase::Unconfirmed { .. }))
    }

    pub fn note(&mut self, s: Sent) {
        self.sent.retain(|x| x.id != s.id);
        self.sent.push(s);
        self.sent.sort_by_key(|x| x.seq);
        if self.sent.len() > KEEP {
            // Never discard words the user still needs to recover.
            if let Some(i) = self.sent.iter().position(|x| x.phase == Phase::Accepted) {
                self.sent.remove(i);
            }
        }
    }

    pub fn state(&self, id: &str) -> Option<State> {
        self.get(id).map(Sent::state)
    }

    pub fn get(&self, id: &str) -> Option<&Sent> {
        self.sent.iter().find(|s| s.id == id)
    }

    pub fn durable(&self, id: &str) -> Option<&Sent> {
        self.sent.iter().find(|s| s.journaled && s.id == id && s.phase != Phase::Accepted)
    }

    pub fn journaled(&self, id: &str) -> bool {
        self.sent.iter().any(|s| s.id == id && s.journaled)
    }

    /// Each poll: a prompt sending longer than [`SLOW_SEND`] says so (`view::footer`); when the
    /// next one will. True when a footer changed.
    pub fn tell_slow(&mut self, now: f64) -> (bool, Option<f64>) {
        let mut changed = false;
        let mut next: Option<f64> = None;
        for s in &mut self.sent {
            if !matches!(s.phase, Phase::Sending { .. }) {
                continue;
            }
            if !machine::step(s, Event::Tick { now }).is_empty() {
                changed = true;
            }
            if let Phase::Sending { slow: SlowSend::Waiting { since }, .. } = s.phase {
                next = Some(next.map_or(since + SLOW_SEND, |n: f64| n.min(since + SLOW_SEND)));
            }
        }
        (changed, next)
    }

    /// Whether [`Self::tell_slow`] would change anything at `now`.
    pub fn slow_writes(&self, now: f64) -> bool {
        self.sent.iter().any(|s| match s.phase {
            Phase::Sending { slow: SlowSend::Unstamped, .. } => true,
            Phase::Sending { slow: SlowSend::Waiting { since }, .. } => now + 1e-9 >= since + SLOW_SEND,
            _ => false,
        })
    }

    /// What [`Self::tell_slow`] returns when it changes nothing: when the next prompt turns slow.
    pub fn slow_next(&self) -> (bool, Option<f64>) {
        let next = self
            .sent
            .iter()
            .filter_map(|s| match s.phase {
                Phase::Sending { slow: SlowSend::Waiting { since }, .. } => Some(since + SLOW_SEND),
                _ => None,
            })
            .reduce(f64::min);
        (false, next)
    }

    /// Whether prompt `id` has been sending past [`SLOW_SEND`].
    pub fn slow(&self, id: &str) -> bool {
        self.get(id).is_some_and(|s| matches!(s.phase, Phase::Sending { slow: SlowSend::Slow, .. }))
    }

    pub fn sending(&self, id: &str) -> bool {
        self.state(id) == Some(State::Unconfirmed)
    }

    /// Takes prompt `id` out of the outbox, its row with it.
    pub fn take(&mut self, id: &str) -> Option<Sent> {
        let at = self.sent.iter().position(|s| s.id == id)?;
        let mut s = self.sent.remove(at);
        self.forget(&mut s);
        Some(s)
    }

    /// A listing dropped `old` items: those still on their way from any of the server's windows
    /// come back, as the server hasn't seen them yet (`multi-2-review.md` #10).
    pub fn preserve_inbox(&self, session: &str, inbox: &mut Inbox, old: Vec<InboxItem>) {
        for item in self.keep_in_inbox(session, old) {
            if !inbox.items.iter().any(|i| i.id == item.id) {
                inbox.upsert(item);
            }
        }
    }

    /// Filters dropped inbox items down to those still in flight for `session`.
    pub fn keep_in_inbox(&self, session: &str, old: Vec<InboxItem>) -> Vec<InboxItem> {
        old.into_iter()
            .filter(|item| {
                self.sent
                    .iter()
                    .any(|s| s.session == session && s.id == item.id && matches!(s.phase, Phase::Sending { .. } | Phase::Unconfirmed { .. } | Phase::Waiting { .. }))
            })
            .collect()
    }

    /// Window `by` closed or left the server with writes out: their replies won't come to any
    /// window, so a receipt check settles them.
    pub fn release(&mut self, by: u64) -> bool {
        let released: Vec<String> = self.sent.iter_mut().filter_map(|s| (!machine::step(s, Event::Released { by }).is_empty()).then(|| s.id.clone())).collect();
        for id in &released {
            self.persist(id, None);
        }
        !released.is_empty()
    }
}

impl Sent {
    pub fn new(session: &str, id: &str, draft: Entry, files: Vec<Attachment>, paths: Vec<PathAttachment>, inbox: bool) -> Self {
        Sent {
            session: session.into(),
            id: id.into(),
            draft,
            files,
            paths,
            inbox,
            prompt: None,
            phase: Phase::Sending { by: 0, then: None, handoff: 0, slow: SlowSend::Unstamped },
            journaled: false,
            operation: Default::default(),
            seq: store::next_seq(),
            row: store::Row::None,
            handoffs: 0,
            left_draft: LeftDraft::None,
        }
    }

    pub fn with_prompt(mut self, prompt: &Prompt) -> Self {
        self.prompt = Some(prompt.clone());
        self
    }

    pub fn with_journaled(mut self, journaled: bool) -> Self {
        self.journaled = journaled;
        self
    }

    pub fn state(&self) -> State {
        match self.phase {
            Phase::Waiting { .. } | Phase::Preparing => State::Waiting,
            Phase::Sending { .. } => State::Sending,
            Phase::Unconfirmed { .. } => State::Unconfirmed,
            Phase::Failed { .. } => State::Failed,
            Phase::Accepted => State::Accepted,
        }
    }

    /// What the phase says, for the bubble's status line.
    pub fn why(&self) -> &str {
        match &self.phase {
            Phase::Waiting { why } | Phase::Unconfirmed { why, .. } | Phase::Failed { why, .. } => why,
            Phase::Sending { .. } | Phase::Accepted | Phase::Preparing => "",
        }
    }

    /// How it ended so far; `None` while it's on its way for the first time.
    pub fn result(&self) -> Option<SendResult> {
        match &self.phase {
            Phase::Accepted => Some(SendResult::Accepted),
            Phase::Failed { .. } => Some(SendResult::Failed),
            Phase::Unconfirmed { .. } => Some(SendResult::Queued),
            Phase::Waiting { why } if !why.is_empty() => Some(SendResult::Queued),
            Phase::Waiting { .. } | Phase::Sending { .. } | Phase::Preparing => None,
        }
    }

    /// The Edit or Delete waiting on the server's answer.
    pub fn then(&self) -> Option<AfterCheck> {
        match self.phase {
            Phase::Sending { then, .. } | Phase::Unconfirmed { then, .. } => then,
            _ => None,
        }
    }

    fn phase_is(&self, state: State) -> bool {
        self.state() == state
    }
}

/// Steps prompt `id`'s machine and carries out what it says.
pub fn step_send(host: &mut dyn SendFailedHost, id: &str, event: Event) -> bool {
    let (before, after, effects) = {
        let Some(s) = host.unsent_mut().sent.iter_mut().find(|s| s.id == id) else { return false };
        let before = s.result();
        let effects = machine::step(s, event);
        let after = s.result();
        (before, after, effects)
    };
    let changed = !effects.is_empty();
    for effect in effects {
        send_effect(host, id, effect);
    }
    if let Some(result) = after.filter(|r| before != Some(*r)) {
        host.on_send_result(id, result);
    }
    changed
}

/// How prompt `id`'s send ended so far.
pub fn send_result(host: &dyn SendFailedHost, id: &str) -> Option<SendResult> {
    host.unsent().get(id).and_then(Sent::result)
}

/// The prompt the outbox took last, if it took one since it held `before`.
pub fn newest_send(host: &dyn SendFailedHost, before: Option<&str>) -> Option<String> {
    host.unsent().sent.last().map(|s| s.id.clone()).filter(|id| Some(id.as_str()) != before)
}

fn send_effect(host: &mut dyn SendFailedHost, id: &str, effect: Effect) {
    match effect {
        Effect::Save => {
            if let Some(op) = host.unsent_mut().persist_op(id, None) {
                host.with_storage(op);
            }
        }
        Effect::Check => check_sends(host),
        Effect::Defer => outbox::defer_outbox(host),
        Effect::Outbox => {
            if let Some(session) = host.unsent().get(id).map(|s| s.session.clone())
                && let Some(i) = host.inboxes_mut().get_mut(&session)
            {
                i.remove(id);
            }
            outbox::show_outbox(host);
        }
        Effect::ShowFailed => fail_send(host, id),
        Effect::Restore => outbox::restore_rejection(host, id),
        Effect::Run(action) => outbox::run_outbox_action(host, id, action),
        Effect::Accepted { late } => {
            if let Some(session) = host.unsent().get(id).filter(|s| s.journaled).map(|s| s.session.clone()) {
                opencode_platform::telemetry::send_receipt(&session, id, "accepted", 0);
                host.unsent_mut().retry = Retry::Ready;
            }
            if late {
                host.toast_say(
                    "Message already sent".into(),
                    "The server received it before your change.".into(),
                );
            }
        }
        Effect::Slow => {}
    }
}

/// A prompt write's error (`Incoming::SendFailed`).
pub fn send_failed(host: &mut dyn SendFailedHost, id: &str, error: &WireError) -> bool {
    let (why, verdict) = {
        let Some(sent) = host.unsent().get(id) else { return false };
        if sent.phase == Phase::Accepted {
            return true;
        }
        eprintln!("brocode: prompt send failed: {error}");
        if sent.journaled {
            let code = error.status();
            opencode_platform::telemetry::send_receipt(&sent.session, id, "write failed", code.unwrap_or(0));
            let not_started = match sent.operation {
                outbox::Operation::Start(_) => error.method == "POST" && error.path == "/api/session",
                outbox::Operation::Worktree { .. } | outbox::Operation::Revert => false,
                _ => true,
            };
            let verdict = if code.is_some_and(|n| (400..500).contains(&n) && !matches!(n, 408 | 425 | 429)) {
                Verdict::Rejected
            } else if code.is_none() && error.failed_for_sure() && not_started {
                Verdict::Unsent
            } else {
                Verdict::Lost
            };
            (reason::words(error).to_owned(), verdict)
        } else {
            (reason::words(error).to_owned(), if error.failed_for_sure() { Verdict::Unsent } else { Verdict::Lost })
        }
    };
    step_send(host, id, Event::WriteFailed { why, verdict });
    true
}

/// A successful read or reconnect retries receipts, with one bounded batch per session.
pub fn check_sends(host: &mut dyn SendFailedHost) {
    start_checks(host, true);
}

/// Pumping must not queue another check every frame while this session's read is out.
pub(super) fn check_sends_if_idle(host: &mut dyn SendFailedHost) {
    start_checks(host, false);
}

fn start_checks(host: &mut dyn SendFailedHost, again: bool) {
    let Some(server) = host.live_server() else { return };
    let mut sessions: HashMap<String, Vec<(String, String)>> = HashMap::new();
    for s in &host.unsent().sent {
        if s.state() == State::Unconfirmed {
            sessions.entry(s.session.clone()).or_default().push((s.session.clone(), s.id.clone()));
        }
    }
    let waker = host.waker().unwrap_or_else(|| Arc::new(|| {}));
    for (session, ask) in sessions {
        let check = host.unsent_mut().checks.entry(session).or_default();
        if let Check::Checking { again: queued } = &mut check.phase {
            *queued |= again;
            continue;
        }
        let server = server.clone();
        check.phase = Check::Checking { again: false };
        check.receipts = rune::Latest::new(waker.clone());
        check.receipts.request(ask, move |ask| read_receipts(&|path| server.read(path), ask));
    }
}

/// Drains storage mail and completed delivery receipt checks. A slow session's read never
/// holds another session's receipts (or new prompts) behind it.
pub fn poll_sends(host: &mut dyn SendFailedHost) -> bool {
    let mut changed = outbox::take_mail(host);
    let ready: Vec<_> = host.unsent_mut().checks.iter_mut()
        .filter_map(|(session, check)| check.receipts.take().map(|r| (session.clone(), r))).collect();
    let mut again = false;
    for (session, receipts) in ready {
        again |= host.unsent_mut().checks.remove(&session).is_some_and(|c| matches!(c.phase, Check::Checking { again: true }));
        apply_receipts(host, receipts);
        changed = true;
    }
    if again { check_sends_if_idle(host); }
    changed
}

fn apply_receipts(host: &mut dyn SendFailedHost, receipts: Vec<(String, Result<receipt::Receipt, WireError>)>) {
    for (id, result) in receipts {
        // A newer SSE receipt always wins over an older absent read.
        if host.unsent().state(&id) != Some(State::Unconfirmed) {
            continue;
        }
        let found = match result {
            Ok(receipt::Receipt::Delivered(message)) => {
                let session = host.unsent().get(&id).map(|s| s.session.clone()).unwrap_or_default();
                host.transcripts_mut().entry(session.clone()).or_insert_with(|| Transcript { session: session.clone(), messages: vec![] }).merge(vec![*message]);
                if let Some(inbox) = host.inboxes_mut().get_mut(&session) {
                    inbox.remove(&id);
                }
                Found::Arrived
            }
            Ok(receipt::Receipt::Admitted(items)) => {
                if let Some(session) = host.unsent().get(&id).map(|s| s.session.clone()) {
                    let dropped: Vec<InboxItem> = {
                        let inbox = host.inboxes_mut().entry(session.clone()).or_default();
                        let previous = std::mem::take(&mut inbox.items);
                        inbox.replace(&items);
                        previous.into_iter().filter(|o| !inbox.items.iter().any(|i| i.id == o.id)).collect()
                    };
                    let kept = host.unsent().keep_in_inbox(&session, dropped);
                    if !kept.is_empty() {
                        let inbox = host.inboxes_mut().entry(session).or_default();
                        for item in kept {
                            if !inbox.items.iter().any(|i| i.id == item.id) {
                                inbox.upsert(item);
                            }
                        }
                    }
                }
                admitted_send(host, &id);
                continue;
            }
            Ok(receipt::Receipt::Missing) => Found::Missing,
            Err(error) if error.status() == Some(404) => Found::NoSession,
            Err(error) => {
                eprintln!("brocode: prompt receipt unavailable: {error}");
                Found::Unknown(reason::words(&error))
            }
        };
        step_send(host, &id, Event::Checked(found));
    }
}

/// Positive evidence is usable even without a preceding reconnect, and from older pages.
/// Absence on a limited latest page is NOT evidence that an older prompt never arrived.
pub fn judge_sends_by_page(host: &mut dyn SendFailedHost, _session: &str, messages: &[Message]) -> bool {
    let mut changed = false;
    for message in messages {
        changed |= accept_send(host, message.id());
    }
    changed
}

/// Marks any outbox prompt present in an inbox listing as admitted.
pub fn judge_sends_by_inbox(host: &mut dyn SendFailedHost, _session: &str, ids: &[String]) -> bool {
    let mut changed = false;
    for id in ids {
        changed |= admitted_send(host, id);
    }
    changed
}

/// An accepted follow-up still waits for the busy session. Move its offline echo
/// into the inbox until delivery; an already delivered message must never move back.
pub fn admitted_send(host: &mut dyn SendFailedHost, id: &str) -> bool {
    let first_op = host
        .unsent()
        .durable(id)
        .and_then(|s| s.operation.first_after().map(|after| (s.session.clone(), after.to_vec())));
    let synced = first_op.as_ref().is_some_and(|(session, _)| host.inboxes().get(session).is_some_and(|i| i.synced));
    if let Some(s) = host.unsent().durable(id).filter(|s| s.inbox).cloned() {
        if let Some(t) = host.transcripts_mut().get_mut(&s.session) {
            t.messages.retain(|m| m.id() != id);
        }
        let inbox = host.inboxes_mut().entry(s.session.clone()).or_default();
        if !inbox.items.iter().any(|item| item.id == id) {
            inbox.upsert(InboxItem {
                id: id.into(),
                text: s.draft.text.clone(),
                files: s.files.clone(),
                delivery: s.prompt.as_ref().and_then(|p| p.delivery).unwrap_or(Delivery::Steer),
            });
        }
    }
    let changed = accept_send(host, id);
    if let Some((session, after)) = first_op {
        host.on_admitted_first(&session, id, after, !synced);
    }
    changed
}

/// Marks prompt `id` as admitted by the server.
pub fn accept_send(host: &mut dyn SendFailedHost, id: &str) -> bool {
    step_send(host, id, Event::Admitted)
}

/// Shows prompt `id` as failed where it was: out of the inbox, its bubble in the timeline.
fn fail_send(host: &mut dyn SendFailedHost, id: &str) {
    let Some(sent) = host.unsent().get(id).cloned() else { return };
    if let Some(i) = host.inboxes_mut().get_mut(&sent.session) {
        i.remove(id);
    }
    let t = host.transcripts_mut().entry(sent.session.clone()).or_insert_with(|| Transcript { session: sent.session.clone(), messages: vec![] });
    if !t.messages.iter().any(|m| m.id() == id) {
        t.messages.push(Message::User { id: id.into(), text: sent.draft.text.clone() });
    }
    host.retain_echoes_without(id);
    if !sent.journaled {
        host.toast_fail(opencode_i18n::t!("prompt.toast.promptSendFailed.title").into(), sent.why().into());
    }
}

/// Retry on a local prompt that failed: the same request, under its own id.
pub fn retry_send(host: &mut dyn SendFailedHost, id: &str) {
    let Some(sent) = host.unsent().get(id).filter(|s| s.phase_is(State::Failed)).cloned() else { return };
    let Some(prompt) = sent.prompt.clone() else { return };
    let write = host.send_write(&sent.session, prompt);
    if !host.is_fixture() && !host.write(write) {
        return;
    }
    if sent.inbox {
        if let Some(t) = host.transcripts_mut().get_mut(&sent.session) {
            t.messages.retain(|m| m.id() != id);
        }
        host.inboxes_mut().entry(sent.session.clone()).or_default().upsert(InboxItem {
            id: id.into(),
            text: sent.draft.text.clone(),
            files: sent.files.clone(),
            delivery: sent.prompt.as_ref().and_then(|p| p.delivery).unwrap_or(Delivery::Steer),
        });
    }
    let by = host.peer_token();
    host.unsent_mut().note(Sent { phase: Phase::Sending { by, then: None, handoff: 0, slow: SlowSend::Unstamped }, ..sent });
}

/// Moves a local failed prompt back into an empty composer for editing.
pub fn edit_failed_send(host: &mut dyn SendFailedHost, id: &str) {
    let Some(sent) = host.unsent().get(id).filter(|s| s.phase_is(State::Failed)).cloned() else { return };
    if !host.is_session_here(&sent.session) || !host.composer().text.is_empty() || host.composer_has_attachments() {
        return;
    }
    outbox::restore_draft(host, sent);
    host.composer_mut().focused = true;
}

/// Each prompt's receipt in turn, stopping at the first the server can't answer.
fn read_receipts(fetch: &dyn Fn(&str) -> Result<serde_json::Value, WireError>, ask: Vec<(String, String)>) -> Vec<(String, Result<receipt::Receipt, WireError>)> {
    let mut results = Vec::new();
    for (session, id) in ask {
        let result = receipt::read(fetch, &session, &id);
        let unavailable = result.is_err();
        results.push((id, result));
        if unavailable {
            break;
        }
    }
    results
}
