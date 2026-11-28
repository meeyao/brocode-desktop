//! The timeline's projection, kept and made incremental. A transcript of ten thousand messages
//! projects in milliseconds, and its region builds again for many reasons that change none of
//! it (scrolling past its end, a poll, Find opening), or change only its end (a streamed word, a
//! tool finishing, a steer waiting).
//!
//! So a long transcript's rows are kept in spans (`span`): the newest span, from the last prompt
//! a turn can't run past, projects on every change; the spans above it project once, and again
//! only when a message in them changes (their stamp, `stamps_of`). A session shown for the
//! first time, its reader at its end, projects its newest spans, enough for its first screens and
//! their overscan, and the rest on the frame after ([`poll`]), so its first frame costs what a
//! short session's does. A page of older messages landing above moves the spans down.
//!
//! What a projection reads besides the messages is in [`Global`] (a change projects every span
//! again) and [`Tail`] (only the newest). A host that can't version its transcripts
//! (`TimelineHost::transcript_version`) projects them whole on every build, as before.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::hash::{BuildHasher, Hash, Hasher};
use std::ops::Range;
use std::rc::Rc;

use opencode_kit::model::{self, Message, NoticeKind, Part, Transcript};

use super::span::{self, Inputs};
use super::{RowSpec, dividers, rhythm};
use crate::TimelineHost;

/// How many messages a span holds at least, above the newest.
const SPAN: usize = 1500;
/// How many messages the first build of a session projects, newest first: its first screens.
const FIRST: usize = 300;

/// A transcript's rows: its spans' rows in order, with the rows where two spans meet spaced
/// against each other (`seams`).
#[derive(Default)]
pub struct Projection {
    spans: Vec<Rc<Vec<RowSpec>>>,
    /// The index of each span's first row.
    starts: Vec<usize>,
    len: usize,
    /// The rows where spans meet, spaced against their neighbours, by index.
    seams: Vec<(usize, RowSpec)>,
    /// Which projection made the rows: a virtual list keeps the keys of the last commit given
    /// the same one (`El::virtual_rows_version`).
    pub generation: u64,
    /// Extra bottom tail room past the 36 pt Working lane (`rhythm::tail_extra`), cached with the
    /// projection so `timeline()` does not re-fold the last turn on every view build.
    pub tail_extra: f32,
}

impl Projection {
    /// `same`: the rows' keys are the previous projection's (`generation`), in the same order.
    /// `older`: messages are still to land above the first row.
    fn new(spans: Vec<Rc<Vec<RowSpec>>>, t: &Transcript, detail: &crate::detail::Detail, script_diffs: bool, same: Option<u64>, older: bool) -> Projection {
        let mut starts = Vec::with_capacity(spans.len());
        let mut len = 0;
        for s in &spans {
            starts.push(len);
            len += s.len();
        }
        let generation = same.unwrap_or_else(|| {
            let g = GENERATION.get() + 1;
            GENERATION.set(g);
            g
        });
        let mut p = Projection { spans, starts, len, seams: Vec::new(), generation, tail_extra: 0.0 };
        p.seams = p.space_seams(t, detail, script_diffs);
        // The first row with messages still to land above takes no room over it: the row that
        // lands above it brings the room between them (`rhythm::space_at`), so the first row is
        // the same row before and after (official `packages/app/AGENTS.md`, "Rows that load
        // above the reader"). The session's first row keeps the room it starts the timeline with.
        if older && let Some(mut first) = p.get(0).cloned() {
            if let Some(top) = rhythm::top_of(&mut first.content) {
                *top = 0.0;
            }
            dividers::wait(&mut first.content);
            match p.seams.first_mut().filter(|(at, _)| *at == 0) {
                Some(seam) => seam.1 = first,
                None => p.seams.insert(0, (0, first)),
            }
        }
        p.tail_extra = rhythm::tail_extra(t, detail, script_diffs, &p);
        p
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn get(&self, i: usize) -> Option<&RowSpec> {
        if let Ok(k) = self.seams.binary_search_by_key(&i, |(at, _)| *at) {
            return Some(&self.seams[k].1);
        }
        self.base(i)
    }

    pub fn iter(&self) -> impl Iterator<Item = &RowSpec> {
        (0..self.len).filter_map(|i| self.get(i))
    }

    /// Row `i` as its span projected it.
    fn base(&self, i: usize) -> Option<&RowSpec> {
        let k = self.starts.partition_point(|&s| s <= i).checked_sub(1)?;
        self.spans[k].get(i - self.starts[k])
    }

    /// Where two spans meet, each span spaced its rows among themselves: the last row of the one
    /// above and the first of the one below learn what's beside them (`dividers`, `rhythm`): the
    /// upper one takes the room between them, which the lower one was built with over it.
    fn space_seams(&self, t: &Transcript, detail: &crate::detail::Detail, script_diffs: bool) -> Vec<(usize, RowSpec)> {
        let mut out: Vec<(usize, RowSpec)> = Vec::new();
        for &seam in self.starts.iter().skip(1).filter(|&&s| s > 0 && s < self.len) {
            let from = seam.saturating_sub(2);
            let to = (seam + 2).min(self.len);
            let mut window: Vec<RowSpec> = (from..to).filter_map(|i| self.base(i).cloned()).collect();
            let at = seam - 1 - from..seam + 1 - from;
            dividers::stack_at(t, &mut window, at.clone());
            rhythm::space_at(t, detail, script_diffs, &mut window, at.clone());
            // A span of one row meets a span on both sides: the seam under it knows its room.
            if out.last().is_some_and(|(i, _)| *i == seam - 1) {
                out.pop();
            }
            out.extend(at.map(|j| (from + j, window[j].clone())));
        }
        out
    }
}

/// What every span reads besides its messages.
#[derive(PartialEq)]
struct Global {
    extras: u64,
    detail: crate::detail::Detail,
    turn_diffs: bool,
    full_codemode: bool,
    fold_user: bool,
    viewport: u32,
    revert: Option<String>,
    /// The device scale the rows were spaced at (`rhythm::prose_insets`): a projection filled
    /// before a window's first view spaced them at 1, so the view that knows the scale respaces.
    scale: u32,
}

/// What only the newest span reads: whether the turn runs, Working, and what waits to send.
#[derive(PartialEq)]
struct Tail {
    older: bool,
    busy: bool,
    live: bool,
    asking: bool,
    actions: Option<usize>,
    queued: Option<String>,
    inbox: u64,
}

/// A span above the newest: its messages, their stamp, and its rows.
struct Kept {
    range: Range<usize>,
    stamp: u64,
    rows: Rc<Vec<RowSpec>>,
}

/// A session's projection as it stands.
struct Session {
    global: Global,
    /// The transcript's version and the newest span's inputs the rows were projected at.
    version: u64,
    tail: Tail,
    /// The spans above the newest, oldest first, running to `newest`'s start; the first starts
    /// below the transcript's start until they're all projected.
    spans: Vec<Kept>,
    newest: (Range<usize>, Rc<Vec<RowSpec>>),
    /// The first projected message's id: where it stands now says how many messages landed
    /// above it (an older page).
    top: Option<String>,
    rows: Rc<Projection>,
    /// A partial projection's spans above, projected a span a frame and landed at once ([`poll`]).
    fill: Fill,
}

/// The spans above a partial projection on their way: each landing costs the timeline's list a
/// pass over all its keys, so they land at once, but a long transcript's take many frames of
/// work to project, so each frame projects one span ([`SPAN`] messages) of them.
#[derive(Default)]
enum Fill {
    #[default]
    Idle,
    /// Projected above `top`, newest first, each stamped as it was projected.
    Climbing { top: usize, spans: Vec<Kept> },
}

impl Session {
    fn top(&self) -> usize {
        self.spans.first().map_or(self.newest.0.start, |s| s.range.start)
    }
}

/// The sessions whose projections are kept: the open one, and a few the window shows beside it
/// (a side pane, a tab just left).
const KEPT: usize = 4;

thread_local! {
    static SESSIONS: RefCell<HashMap<String, Session>> = RefCell::new(HashMap::new());
    /// Each projection's number; 0 is the empty one.
    static GENERATION: Cell<u64> = const { Cell::new(0) };
    /// Moves when a partial projection was filled in, so the timeline's region builds again.
    static WANTED: Cell<u64> = const { Cell::new(0) };
}

/// Transcript `t`'s rows: kept while what they read holds, projected only where it doesn't.
pub fn rows<S: TimelineHost>(s: &S, t: &Transcript) -> Rc<Projection> {
    let whole = |s: &S| Rc::new(Projection::new(vec![Rc::new(super::project(s, t))], t, &s.timeline_detail(), s.full_codemode(&t.session), None, s.more_above(&t.session)));
    let Some(version) = s.transcript_version(&t.session) else { return whole(s) };
    let fold_user = s.fold_user_messages();
    // An opened prompt folds by where it stands on screen (`fold::place`): its rows move with the
    // scroll, so they aren't kept.
    if fold_user && s.expanded().iter().any(|k| k.starts_with("user-expanded:")) {
        SESSIONS.with_borrow_mut(|m| m.remove(&t.session));
        return whole(s);
    }
    let inp = Inputs::read(s, t);
    let global = Global {
        extras: model::prompt_extras_generation(),
        detail: inp.detail,
        turn_diffs: inp.turn_diffs,
        full_codemode: inp.full_codemode,
        fold_user,
        viewport: crate::fold::place(s, &t.session)("").viewport.to_bits(),
        revert: inp.revert.map(str::to_owned),
        scale: crate::scroller::device_scale().to_bits(),
    };
    let tail = tail_of(s, t, &inp);
    // Only a reader at the end can start with the newest rows: one scrolled up needs the row it
    // stands on.
    let partial = s.ends().at_end(&t.session);
    let kept = SESSIONS.with_borrow_mut(|m| m.remove(&t.session));
    let session = match kept {
        Some(k) if k.global == global && k.version == version && k.tail == tail => k,
        Some(k) if k.global == global => update(s, t, &inp, k, version, tail).unwrap_or_else(|g| first(s, t, &inp, g, version, tail_of(s, t, &inp), false)),
        Some(_) => first(s, t, &inp, global, version, tail, false),
        None => first(s, t, &inp, global, version, tail, partial),
    };
    let rows = session.rows.clone();
    SESSIONS.with_borrow_mut(|m| {
        if m.len() >= KEPT {
            m.clear();
        }
        m.insert(t.session.clone(), session);
    });
    rows
}

fn tail_of<S: TimelineHost>(s: &S, t: &Transcript, inp: &Inputs) -> Tail {
    Tail {
        older: inp.older,
        busy: inp.busy,
        live: s.is_session_live(&t.session),
        asking: s.session_asking(&t.session),
        actions: inp.actions,
        queued: inp.queued.map(str::to_owned),
        inbox: inbox_hash(s, &t.session),
    }
}

/// A session's projection afresh: its newest span, then older ones, all of them, or (`partial`)
/// until [`FIRST`] messages show, [`poll`] projecting the rest on the next frame.
#[allow(clippy::too_many_arguments)]
fn first<S: TimelineHost>(s: &S, t: &Transcript, inp: &Inputs, global: Global, version: u64, tail: Tail, partial: bool) -> Session {
    let messages = &t.messages[..inp.shown];
    let start = newest_start(messages, 0);
    let newest = (start..inp.shown, Rc::new(span::project(s, t, inp, start..inp.shown, true)));
    let mut spans = Vec::new();
    let mut top = start;
    while top > 0 && (!partial || inp.shown - top < FIRST) {
        let range = older(messages, top, if partial { FIRST } else { SPAN });
        top = range.start;
        spans.push(Kept { stamp: 0, rows: Rc::new(span::project(s, t, inp, range.clone(), false)), range });
    }
    spans.reverse();
    stamp_all(t, inp, &mut spans);
    let top = messages.get(top).map(|m| m.id().to_owned());
    assemble(t, inp, Session { global, version, tail, spans, newest, top, rows: Rc::default(), fill: Fill::Idle })
}

/// The transcript or the newest span's inputs changed: the spans above whose messages didn't
/// change keep their rows, the newest projects again, and new prompts start spans of their own.
/// A page landing above moves the spans down; messages gone or replaced under them (`Err`)
/// project the transcript afresh.
fn update<S: TimelineHost>(s: &S, t: &Transcript, inp: &Inputs, mut k: Session, version: u64, tail: Tail) -> Result<Session, Global> {
    let messages = &t.messages[..inp.shown];
    if k.version != version {
        // Messages that landed above the projected ones (an older page) move them down.
        let top = k.top();
        let moved = match &k.top {
            Some(id) => (top..messages.len()).find(|&i| messages[i].id() == id).map(|i| i - top),
            None => Some(0),
        };
        let Some(by) = moved else { return Err(k.global) };
        if by > 0 {
            k.fill = Fill::Idle;
            for sp in &mut k.spans {
                sp.range = sp.range.start + by..sp.range.end + by;
                sp.rows = Rc::new(sp.rows.iter().map(|r| shifted(r, by)).collect());
            }
            k.newest.0 = k.newest.0.start + by..k.newest.0.end + by;
            // A page that landed above a whole projection is the span above it.
            if top == 0 {
                let end = k.spans.first().map_or(k.newest.0.start, |sp| sp.range.start);
                if !span::boundary(messages, end) {
                    return Err(k.global);
                }
                let mut page = vec![Kept { stamp: 0, rows: Rc::new(span::project(s, t, inp, 0..end, false)), range: 0..end }];
                stamp_all(t, inp, &mut page);
                page.append(&mut k.spans);
                k.spans = page;
            }
        }
        if k.spans.iter().any(|sp| sp.range.end > messages.len() || !span::boundary(messages, sp.range.end) || (sp.range.start > 0 && !span::boundary(messages, sp.range.start))) {
            return Err(k.global);
        }
        // A streamed reply's writes say where they changed messages (`transcript_changed_from`):
        // the spans that read only messages above that keep their stamps, so a word streaming
        // into a long transcript compares its own span, not every message above it.
        let from = if by > 0 { 0 } else { s.transcript_changed_from(&t.session, k.version).unwrap_or(0) };
        let mut stamps: Vec<u64> = k.spans.iter().map(|sp| sp.stamp).collect();
        let ranges: Vec<Range<usize>> = k.spans.iter().map(|s| s.range.clone()).collect();
        stamps_of(t, inp, &ranges, from, &mut stamps);
        for (sp, stamp) in k.spans.iter_mut().zip(stamps) {
            if sp.stamp != stamp {
                sp.rows = Rc::new(span::project(s, t, inp, sp.range.clone(), false));
                sp.stamp = stamp;
            }
        }
        k.top = messages.get(k.top()).map(|m| m.id().to_owned());
    }
    // Whether older messages remain above is the transcript's first span's to read (its first
    // row starts the session, or stands under what is still to land): it projects again when
    // that changes, whether or not a message did. Kept, a newest page's first prompt stayed the
    // session's start (8 over it) under the page that later landed above it.
    if k.tail.older != tail.older && let Some(sp) = k.spans.first_mut().filter(|sp| sp.range.start == 0) {
        sp.rows = Rc::new(span::project(s, t, inp, sp.range.clone(), false));
        stamp_all(t, inp, &mut k.spans[..1]);
    }
    // The newest span starts where the spans above end, and splits at each prompt since.
    let mut from = k.spans.last().map_or(k.newest.0.start, |sp| sp.range.end);
    if from > messages.len() || (from > 0 && !span::boundary(messages, from)) {
        return Err(k.global);
    }
    let start = newest_start(messages, from);
    let mut added = Vec::new();
    while from < start {
        let end = (from + 1..=start).find(|&c| c == start || (c - from >= SPAN && span::boundary(messages, c))).unwrap_or(start);
        added.push(Kept { stamp: 0, rows: Rc::new(span::project(s, t, inp, from..end, false)), range: from..end });
        from = end;
    }
    stamp_all(t, inp, &mut added);
    k.spans.extend(added);
    k.newest = (start..inp.shown, Rc::new(span::project(s, t, inp, start..inp.shown, true)));
    k.version = version;
    k.tail = tail;
    if k.spans.is_empty() {
        k.top = messages.get(start).map(|m| m.id().to_owned());
    }
    Ok(assemble(t, inp, k))
}

/// Row `r` of messages that moved down `by`: the same row, at the messages' new places.
fn shifted(r: &RowSpec, by: usize) -> RowSpec {
    use super::RowContent::*;
    let mut r = r.clone();
    match &mut r.content {
        User { message, .. } | Error { message, .. } | Retry { message, .. } | Compaction { message, .. } | TurnText { message, .. } | Actions { message, .. } => *message += by,
        Notice(i, ..) => *i += by,
        Turn { start, end, .. } | ToolImages { start, end, .. } => {
            *start += by;
            *end += by;
        }
        TurnDiff { idle, .. } => *idle += by,
        Pending { .. } | PendingNotice(..) | CompactionQueued(_) | TurnDivider(_) | UserImages { .. } => {}
    }
    r
}

/// Projects one more span above a partial projection; with the climb at the transcript's top,
/// lands them all at once (each landing costs the timeline's list a pass over all its keys).
/// Returns the session and whether the spans landed.
fn fill<S: TimelineHost>(s: &S, t: &Transcript, inp: &Inputs, mut k: Session) -> (Session, bool) {
    let messages = &t.messages[..inp.shown];
    let (mut top, mut climbed) = match std::mem::take(&mut k.fill) {
        Fill::Climbing { top, spans } if top <= k.top() => (top, spans),
        _ => (k.top(), Vec::new()),
    };
    if top > 0 {
        let range = older(messages, top, SPAN);
        top = range.start;
        let mut sp = vec![Kept { stamp: 0, rows: Rc::new(span::project(s, t, inp, range.clone(), false)), range }];
        stamp_all(t, inp, &mut sp);
        climbed.extend(sp);
    }
    if top > 0 {
        k.fill = Fill::Climbing { top, spans: climbed };
        return (k, false);
    }
    climbed.reverse();
    climbed.append(&mut k.spans);
    k.spans = climbed;
    k.top = messages.get(k.top()).map(|m| m.id().to_owned());
    (assemble(t, inp, k), true)
}

/// The session's rows from its spans. While a reply streams only its newest span projects again,
/// usually to the same rows: the projection then keeps its generation, so the timeline's list
/// keeps the keys it holds instead of asking each row's again.
fn assemble(t: &Transcript, inp: &Inputs, mut k: Session) -> Session {
    let spans: Vec<Rc<Vec<RowSpec>>> = k.spans.iter().map(|sp| sp.rows.clone()).chain(std::iter::once(k.newest.1.clone())).collect();
    let old = &k.rows.spans;
    let same = (old.len() == spans.len()
        && old.iter().zip(&spans).rev().skip(1).all(|(a, b)| Rc::ptr_eq(a, b))
        && old.last().zip(spans.last()).is_some_and(|(a, b)| a.len() == b.len() && a.iter().zip(b.iter()).all(|(x, y)| x.key == y.key)))
        .then_some(k.rows.generation);
    k.rows = Rc::new(Projection::new(spans, t, &inp.detail, inp.full_codemode, same, inp.older || k.top() > 0));
    k
}

/// Where the newest span starts: the last prompt after `from` a span can start at, else `from`.
fn newest_start(messages: &[Message], from: usize) -> usize {
    (from + 1..messages.len()).rev().find(|&c| span::boundary(messages, c)).unwrap_or(from)
}

/// The span just above message `top`: from the latest prompt a span can start at, at or above
/// `size` messages up, else from the transcript's start.
fn older(messages: &[Message], top: usize, size: usize) -> Range<usize> {
    let want = top.saturating_sub(size);
    let start = (1..=want).rev().find(|&c| span::boundary(messages, c)).unwrap_or(0);
    start..top
}
fn stamp_all(t: &Transcript, inp: &Inputs, spans: &mut [Kept]) {
    let ranges: Vec<Range<usize>> = spans.iter().map(|s| s.range.clone()).collect();
    let mut stamps = vec![0; ranges.len()];
    stamps_of(t, inp, &ranges, 0, &mut stamps);
    for (sp, stamp) in spans.iter_mut().zip(stamps) {
        sp.stamp = stamp;
    }
}

fn hasher() -> impl Hasher {
    std::hash::BuildHasherDefault::<rustc_hash::FxHasher>::default().build_hasher()
}

/// Each span's stamp: everything about its messages its rows show, the prompts that start the
/// next span, and the run under way where it starts. A message changing in a way a row shows
/// changes its span's stamp; one that doesn't (a word of a settled reply is drawn from the
/// transcript when the row builds) needn't. Spans that read only messages before `from` (none of
/// them changed) keep the stamp `out` holds.
fn stamps_of(t: &Transcript, inp: &Inputs, ranges: &[Range<usize>], from: usize, out: &mut [u64]) {
    let messages = &t.messages[..inp.shown];
    for (k, r) in ranges.iter().enumerate() {
        // A span's rows read the prompts that start the next one (the turn above runs to their
        // end), the run under way where it starts, and, the transcript's first, whether older
        // messages remain above it: a span that stood first and no longer does projects again.
        let end = if r.end < messages.len() { super::delivery::end(&[], messages, r.end) } else { r.end };
        if end <= from {
            continue;
        }
        let mut h = hasher();
        (span::run_at(messages, r.start), (r.start == 0).then_some(inp.older)).hash(&mut h);
        for m in &messages[r.start..end.min(messages.len())] {
            shape(m, &inp.detail, inp.full_codemode).hash(&mut h);
        }
        out[k] = h.finish();
    }
}

/// What a row shows of message `m`, hashed: its kind and id, what makes its parts show or not
/// and how they fold, and nothing of what the row draws from it as it builds.
fn shape(m: &Message, detail: &crate::detail::Detail, script_diffs: bool) -> u64 {
    let mut h = hasher();
    let step = |h: &mut _, a: &model::Assistant| {
        (&a.id, a.completed.is_some(), a.error.is_some(), a.retry.is_some(), a.failure.as_ref().map(|f| super::interrupted(&f.kind)), &a.changed, a.parts.len()).hash(h);
        for p in &a.parts {
            match p {
                Part::Text { text } => (0u8, text.len(), text.trim().is_empty()).hash(h),
                Part::Reasoning { text, .. } => (1u8, text.trim().is_empty()).hash(h),
                Part::Tool { id, name, status, input, metadata, files, .. } => {
                    (2u8, id, name, std::mem::discriminant(status), crate::detail::failed(name, status, metadata), metadata["jobID"].as_str(), crate::images::shows(files)).hash(h);
                    // A script's changes each place as a step of their own.
                    if script_diffs && detail.placement(crate::detail::Category::Edit) != crate::detail::Placement::Hidden {
                        crate::activity::rules::script_changes(name, status, metadata).count().hash(h);
                    }
                    // How a call joins the calls beside it: a read's path, a skill's name.
                    (crate::tools::path(input), metadata["loaded"].as_array().map(Vec::len), input["name"].as_str()).hash(h);
                }
            }
        }
    };
    match m {
        Message::User { id, text } => (0u8, id, text.trim().is_empty()).hash(&mut h),
        Message::Assistant(a) => {
            1u8.hash(&mut h);
            step(&mut h, a);
        }
        Message::Note { id, step: s, .. } => {
            (2u8, id).hash(&mut h);
            step(&mut h, s);
        }
        Message::Notice(n) => (3u8, &n.id, std::mem::discriminant(&n.kind), crate::detail::notice_placement(n, detail), crate::turn_errors::is_divider(n)).hash(&mut h),
        Message::Compaction(c) => (4u8, &c.id, crate::compaction::pieces(c).iter().map(|p| p.key("")).collect::<Vec<_>>()).hash(&mut h),
    }
    h.finish()
}

fn inbox_hash<S: TimelineHost>(s: &S, session: &str) -> u64 {
    let mut h = hasher();
    if let Some(i) = s.inbox_for(session) {
        for item in &i.items {
            (&item.id, &item.text, std::mem::discriminant(&item.delivery), item.files.len()).hash(&mut h);
        }
        i.compaction.hash(&mut h);
        for n in i.waiting_notices() {
            (&n.id, matches!(n.kind, NoticeKind::Job { .. })).hash(&mut h);
        }
    }
    h.finish()
}

/// Where the timeline's region learns a partial projection was filled in: an input of the
/// region that moves when it is.
pub fn wanted() -> u64 {
    WANTED.get()
}

/// Has the open session's partial projection project the rest, and asks for the next frame,
/// whose build of the timeline shows it ([`wanted`]).
pub fn poll<S: TimelineHost>(s: &S, tx: &mut rune::Tx) -> bool {
    let Some(id) = s.active_session_id() else { return false };
    let version = s.transcript_version(id);
    // A transcript that changed since its timeline last built projects in its next build.
    let due = SESSIONS.with_borrow(|m| m.get(id).is_some_and(|k| k.top() > 0 && Some(k.version) == version));
    if !due {
        return false;
    }
    let filled = s.with_transcript(id, |t| {
        let Some(k) = SESSIONS.with_borrow_mut(|m| m.remove(id)) else { return false };
        let inp = Inputs::read(s, t);
        let (k, landed) = fill(s, t, &inp, k);
        SESSIONS.with_borrow_mut(|m| m.insert(id.to_owned(), k));
        landed
    });
    // Climbing: the next frame projects the next span; nothing shows changed yet.
    tx.wake_after(std::time::Duration::ZERO);
    if filled != Some(true) {
        return false;
    }
    WANTED.set(WANTED.get() + 1);
    true
}
