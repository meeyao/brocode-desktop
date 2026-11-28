//! A transcript's rows, projected a span of messages at a time. A span starts at a prompt the
//! turn before it can't run past ([`boundary`]), so its rows depend only on its own messages,
//! the run under way where it starts ([`run_at`]) and the prompts that begin the next span:
//! [`super::projection`] keeps the spans of a long transcript and projects only the ones that
//! changed, newest first.

use std::collections::BTreeSet;
use std::ops::Range;

use opencode_kit::model::{self, InboxItem, Message, Transcript};

use super::{RowContent, RowSpec, delivery, dividers, entries, interrupted, prompt_images, prompts_join, rhythm, turn_rows};
use crate::TimelineHost;
use crate::compaction::Run;
use crate::detail::{Detail, Placement, notice_placement};
use crate::turns::Entry;

/// What a projection reads from the host, read once for all its spans.
pub(super) struct Inputs<'a> {
    pub busy: bool,
    /// A staged revert's boundary: it hides its message, all after it and the waiting steers.
    pub revert: Option<&'a str>,
    /// The messages shown: those before the revert's boundary.
    pub shown: usize,
    pub pending: Vec<&'a InboxItem>,
    pub queued: Option<&'a str>,
    /// The message Working and the turn's footer belong to.
    pub actions: Option<usize>,
    pub runs_off: bool,
    pub older: bool,
    pub detail: Detail,
    pub turn_diffs: bool,
    pub full_codemode: bool,
}

impl<'a> Inputs<'a> {
    pub fn read<S: TimelineHost>(s: &'a S, t: &Transcript) -> Inputs<'a> {
        let busy = s.timeline_busy(t) && !s.is_session_stopped(&t.session);
        let revert = s.session_revert(&t.session);
        let shown = t.visible_len(revert);
        // A later prompt owns the tail; the previous reply already has its inline footer
        // (`packages/session-ui/src/timeline-next/projection.ts`, per-turn `turnRows`).
        // Notices, compaction and Changed files do not start a new prompt.
        let last_turn = t.messages[..shown].iter().rposition(|m| matches!(m, Message::Assistant(_) | Message::User { .. }));
        let last_assistant = last_turn.filter(|&i| matches!(t.messages[i], Message::Assistant(_)));
        let pending = if revert.is_some() { Vec::new() } else { s.inbox_for(&t.session).map(|i| i.steering().collect()).unwrap_or_default() };
        // Working, or a compaction waiting to start, sits between the last prompt and the steers.
        let queued = if revert.is_none() { s.queued_compaction(&t.session) } else { None };
        // A new session's first prompt has no reply above it yet, and Working shows under it all the same.
        let actions = last_assistant.or_else(|| ((busy || s.is_session_live(&t.session)) && revert.is_none() && shown > 0).then(|| last_turn.unwrap_or(shown - 1)));
        Inputs {
            busy,
            revert,
            shown,
            pending,
            queued,
            actions,
            runs_off: actions.is_some() || queued.is_some(),
            // Not the session's start while older messages remain to land above.
            older: s.more_above(&t.session),
            detail: s.timeline_detail(),
            turn_diffs: s.turn_diffs(),
            full_codemode: s.full_codemode(&t.session),
        }
    }
}

/// Whether a span can start at prompt `c` of `messages`: the first of its run of prompts, after
/// a message no run of prompts takes in (`delivery::end`), and with no job report after its run,
/// which the turn before it would claim.
pub(super) fn boundary(messages: &[Message], c: usize) -> bool {
    let job = |m: Option<&Message>| matches!(m, Some(Message::Notice(model::Notice { kind: model::NoticeKind::Job { .. }, .. })));
    c > 0
        && matches!(messages.get(c), Some(Message::User { .. }))
        && !matches!(messages[c - 1], Message::User { .. })
        && !job(Some(&messages[c - 1]))
        && !job(messages.get(delivery::end(&[], messages, c)))
}

/// The run under way where message `c` starts (`execution`): its first prompt since the last run
/// ended, and the files its steps changed since.
pub(super) fn run_at(messages: &[Message], c: usize) -> Option<(String, BTreeSet<String>)> {
    let from = messages[..c].iter().rposition(|m| matches!(m, Message::Notice(n) if n.kind == model::NoticeKind::Idle)).map_or(0, |i| i + 1);
    let first = (from..c).find(|&i| matches!(messages[i], Message::User { .. }))?;
    let Message::User { id, .. } = &messages[first] else { return None };
    let files = messages[first..c].iter().filter_map(|m| match m {
        Message::Assistant(a) => Some(a.changed.iter().cloned()),
        _ => None,
    }).flatten().collect();
    Some((id.clone(), files))
}

/// The rows of messages `range` of `t`, spaced among themselves. A span that isn't the `last`
/// ends where the next starts ([`boundary`]): the turn before it ends there, and what follows the
/// transcript's end (Working, the waiting steers) is the last span's.
pub(super) fn project<S: TimelineHost>(s: &S, t: &Transcript, inp: &Inputs, range: Range<usize>, last: bool) -> Vec<RowSpec> {
    let mut out = Vec::new();
    let shown = inp.shown;
    let messages = &t.messages[..shown];
    let detail = &inp.detail;
    let script_diffs = inp.full_codemode;
    // Rows stand above a span that doesn't start the transcript.
    let base = range.start > 0;
    let older = inp.older || base;
    let fold = crate::fold::place(s, &t.session);
    // A turn's consecutive steps fold together (see `turns`), with the notices Timeline detail
    // groups; prompts, notes and notices on their own rows end them, and hidden notices don't.
    let mut steps: Vec<Entry> = Vec::new();
    let mut start = range.start;
    // `steps` began with a shell command, whose turn holds only it and its notices.
    let mut shell_turn = false;
    // Each shell command's turn: where its first row goes, its message and the room over it.
    let mut shell_gaps: Vec<(usize, usize, f32)> = Vec::new();
    // The run under way (`execution`): its first prompt and the files its steps changed, and
    // where the last run's Changed files went.
    let mut run = if base { run_at(messages, range.start) } else { None };
    let mut diff_at: Option<usize> = None;
    let mut at = messages[range.clone()].iter().enumerate().map(|(i, m)| (range.start + i, m)).peekable();
    while let Some((mi, m)) = at.next() {
        match m {
            Message::Assistant(a) => {
                // A reply doesn't join a shell command's turn.
                if std::mem::take(&mut shell_turn) {
                    turn_rows(&mut out, &std::mem::take(&mut steps), start, mi, false, detail, script_diffs);
                }
                if let Some(r) = &mut run {
                    r.1.extend(a.changed.iter().cloned());
                }
                if steps.is_empty() { start = mi; }
                steps.push(Entry::Step(a));
                // Steps after an interrupted one (the run went on after a stop) sit under an
                // Interrupted divider: the user stopped the turn, so the timeline confirms it
                // whatever the setting (`timeline-next/projection.ts`'s `turnRows`).
                if a.failure.as_ref().is_some_and(|f| interrupted(&f.kind)) && matches!(messages.get(mi + 1), Some(Message::Assistant(_))) {
                    turn_rows(&mut out, &std::mem::take(&mut steps), start, mi + 1, false, detail, script_diffs);
                    out.push(RowSpec { key: format!("d-{}", a.id), content: RowContent::TurnDivider(Run::default()), streaming: false, under: 0.0, divider: None });
                }
            }
            // A run ends: with Changed files on, the files it changed list after its work.
            Message::Notice(n) if n.kind == model::NoticeKind::Idle => match run.take().filter(|r| inp.turn_diffs && !r.1.is_empty()) {
                Some((from, files)) => {
                    turn_rows(&mut out, &std::mem::take(&mut steps), start, mi, false, detail, script_diffs);
                    diff_at = Some(out.len());
                    out.push(RowSpec { key: format!("td-{}", n.id), content: RowContent::TurnDiff { idle: mi, from, files: files.into_iter().collect(), pt: 16.0 }, streaming: false, under: 0.0, divider: None });
                }
                None if !steps.is_empty() => steps.push(Entry::Idle),
                None => {}
            },
            Message::Notice(n) if !steps.is_empty() || crate::detail::notice_folds(n, detail) => {
                match notice_placement(n, detail) {
                    _ if crate::detail::notice_folds(n, detail) => {
                        if steps.is_empty() { start = mi; }
                        steps.push(Entry::Notice(n));
                    }
                    Placement::Hidden => steps.push(Entry::Skip),
                    _ => {
                        turn_rows(&mut out, &std::mem::take(&mut steps), start, mi, false, detail, script_diffs);
                        out.push(RowSpec { key: format!("n-{}", n.id), content: RowContent::Notice(mi, Run::default(), rhythm::NOTICE_PT), streaming: false, under: 0.0, divider: None });
                    }
                }
            }
            Message::Notice(n) => {
                if notice_placement(n, detail) == Placement::Separate {
                    out.push(RowSpec { key: format!("n-{}", n.id), content: RowContent::Notice(mi, Run::default(), rhythm::NOTICE_PT), streaming: false, under: 0.0, divider: None });
                }
            }
            Message::User { id, .. } => {
                run.get_or_insert_with(|| (id.clone(), Default::default()));
                let end = delivery::end(&steps, messages, mi);
                // Keep source indices: the virtualizer rebuilds each group from this slice.
                if !steps.is_empty() { steps.extend(entries(&messages[mi..end], detail)); }
                turn_rows(&mut out, &std::mem::take(&mut steps), start, end, false, detail, script_diffs);
                delivery::rows(&mut out, messages, mi..end, older, detail, &fold, inp.pending.first().copied().filter(|_| !inp.runs_off));
                while at.next_if(|(i, _)| *i < end).is_some() {}
            }
            // A shell command run from the composer is a turn of its own: its shell call, on its
            // own row or in a group as Timeline detail places shells, with the notices that
            // group after it (`projection.ts` Shell rows, `groupMessages`). It shows unless
            // Shell is hidden, and a failed one always.
            Message::Note { step, .. } => {
                turn_rows(&mut out, &std::mem::take(&mut steps), start, mi, false, detail, script_diffs);
                // Desktop's TurnGap over every turn but the first (16), whose row sits 8 lower
                // than a turn's rows under a prompt.
                shell_gaps.push((out.len(), mi, if out.is_empty() && !base { 8.0 } else { 16.0 }));
                start = mi;
                steps.push(Entry::Step(step));
                shell_turn = true;
            }
            Message::Compaction(c) => {
                turn_rows(&mut out, &std::mem::take(&mut steps), start, mi, false, detail, script_diffs);
                for piece in crate::compaction::pieces(c) {
                    out.push(RowSpec { key: piece.key(&c.id), content: RowContent::Compaction { message: mi, piece, run: Run::default() }, streaming: false, under: 0.0, divider: None });
                }
            }
        }
    }
    if !last {
        // The next span's first prompt ends this span's turn, as it would mid-transcript.
        let end = delivery::end(&steps, messages, range.end);
        if !steps.is_empty() { steps.extend(entries(&messages[range.end..end], detail)); }
        turn_rows(&mut out, &steps, start, end, false, detail, script_diffs);
        space_shells(&mut out, shell_gaps);
        dividers::stack(t, &mut out);
        rhythm::space(t, detail, script_diffs, &mut out);
        return out;
    }
    turn_rows(&mut out, &steps, start, shown, inp.busy && inp.revert.is_none(), detail, script_diffs);
    space_shells(&mut out, shell_gaps);
    if let Some(last) = inp.actions {
        // Working lasts the whole run, like Desktop's session status: each step's message
        // finishes before the next one starts, and those gaps used to blink it out.
        let busy = inp.busy || s.is_session_live(&t.session);
        let actions = RowSpec { key: "turn-actions".into(), content: RowContent::Actions { message: last, busy, asking: busy && s.session_asking(&t.session), last: false }, streaming: false, under: 0.0, divider: None };
        // The reply's footer stays with its words, above the run's Changed files.
        match diff_at.filter(|at| at + 1 == out.len()) {
            Some(at) => {
                out.insert(at, actions);
                // Desktop's footer ends at its Copy (`text-part-copy-wrapper`, 8 over a 20 high
                // row); the lane reserved for Working runs on 8 under it, which Changed files takes
                // from its own 16.
                if let Some(RowSpec { content: RowContent::TurnDiff { pt, .. }, .. }) = out.get_mut(at + 1) {
                    *pt -= crate::working::LANE - crate::working::COPY_AT.1 - crate::working::COPY;
                }
            }
            None => out.push(actions),
        }
    }
    // A compaction waiting to start sits under Working, above the steers, as Desktop's.
    if let Some(id) = inp.queued {
        out.push(RowSpec { key: format!("cmpq-{id}"), content: RowContent::CompactionQueued(Run::default()), streaming: false, under: 0.0, divider: None });
    }
    // Undelivered steers wait under Working, as Desktop shows them, in the row each becomes
    // when the server delivers it to the end of the transcript; Rune slides it up into place.
    let mut prev = match messages.last() {
        Some(Message::User { text, .. }) if steps.is_empty() && !inp.runs_off => Some(text.as_str()),
        _ => None,
    };
    // Background shells and subagents that came back wait among them, in the order the server
    // admitted them (`visibleTimelineMessages`), each a notice of its own.
    let notices: Vec<&model::Notice> = match s.inbox_for(&t.session) {
        Some(i) if inp.revert.is_none() => i.waiting_notices().filter(|n| notice_placement(n, detail) != Placement::Hidden).collect(),
        _ => Vec::new(),
    };
    // Job reports belong to work already started, not to the steers waiting to run.
    let (jobs, notices): (Vec<_>, Vec<_>) = notices.into_iter().partition(|n| matches!(n.kind, model::NoticeKind::Job { .. }));
    for n in jobs {
        out.push(RowSpec { key: format!("n-{}", n.id), content: RowContent::PendingNotice(n.id.clone(), rhythm::NOTICE_PT), streaming: false, under: 0.0, divider: None });
        prev = None;
    }
    let pending = &inp.pending;
    let mut notices = notices.into_iter().peekable();
    for (k, item) in pending.iter().enumerate() {
        while let Some(n) = notices.next_if(|n| n.id < item.id) {
            out.push(RowSpec { key: format!("n-{}", n.id), content: RowContent::PendingNotice(n.id.clone(), rhythm::NOTICE_PT), streaming: false, under: 0.0, divider: None });
            prev = None;
        }
        let above = prev.is_some_and(|x| prompts_join(x, item));
        let below = pending.get(k + 1).is_some_and(|n| prompts_join(&item.text, n)) && notices.peek().is_none_or(|n| n.id > pending[k + 1].id);
        let top = prompt_images(&mut out, &item.id, older, above);
        out.push(RowSpec { key: format!("u-{}", item.id), content: RowContent::Pending { id: item.id.clone(), above, below, top, fold: fold(&item.id) }, streaming: false, under: 0.0, divider: None });
        prev = Some(&item.text);
    }
    for n in notices {
        out.push(RowSpec { key: format!("n-{}", n.id), content: RowContent::PendingNotice(n.id.clone(), rhythm::NOTICE_PT), streaming: false, under: 0.0, divider: None });
    }
    dividers::stack(t, &mut out);
    rhythm::space(t, detail, script_diffs, &mut out);
    // Input waiting to send (queued, or steers) lands as the turn ends, so the reply's footer
    // takes its room only once nothing waits.
    let waiting = s.inbox_for(&t.session).is_some_and(|i| !i.items.is_empty() || i.compaction.is_some());
    if let Some(RowSpec { content: RowContent::Actions { last, .. }, .. }) = out.last_mut() {
        *last = !waiting;
    }
    out
}

/// Each shell command's turn takes the room Desktop's TurnGap gives it.
fn space_shells(out: &mut [RowSpec], gaps: Vec<(usize, usize, f32)>) {
    for (at, mi, gap) in gaps {
        if let Some(RowSpec { content: RowContent::Turn { start, pt, .. }, .. }) = out.get_mut(at)
            && *start == mi
        {
            *pt = gap;
        }
    }
}
