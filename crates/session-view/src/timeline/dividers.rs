//! Dividers back to back sit as one tight group, and a group sits the turn's usual gap from the
//! rows around it (p265): each divider row learns what's above and below it, and the row above a
//! divider learns to hold the room over the divider's words (`compaction::Over`).

use opencode_kit::model::{Message, Transcript};

use super::{RowContent, RowSpec};
use crate::TimelineHost;
use crate::compaction::{Beside, Over, Run};

/// Notes on each divider row in `out` what sits beside it.
pub(super) fn stack(t: &Transcript, out: &mut [RowSpec]) {
    let shown: Vec<usize> = (0..out.len()).collect();
    let kinds: Vec<Kind> = shown.iter().map(|&i| kind(t, &out[i].content)).collect();
    for (k, &i) in shown.iter().enumerate() {
        if kinds[k] != Kind::Divider {
            continue;
        }
        let above = match k.checked_sub(1).map(|j| kinds[j]) {
            Some(Kind::Divider) => Beside::Divider,
            Some(Kind::Prompt) => Beside::Spaced,
            _ => Beside::Other,
        };
        let below = match kinds.get(k + 1) {
            Some(Kind::Divider) => Beside::Divider,
            Some(Kind::Prompt | Kind::Roomy) | None => Beside::Spaced,
            Some(Kind::Other) => Beside::Other,
        };
        // The row above holds the room over the divider's words (`Over`).
        let over = if k > 0 { Over::Above } else { Over::Own };
        let run = Run { above, below, over };
        match &mut out[i].content {
            RowContent::Notice(_, r, _) | RowContent::Compaction { run: r, .. } | RowContent::CompactionQueued(r) | RowContent::TurnDivider(r) => *r = run,
            _ => {}
        }
        if k > 0 {
            out[shown[k - 1]].divider = Some(above);
        }
    }
}

/// [`stack`] for rows `at` alone, against the rows beside them: where two spans of a projection
/// meet (`projection`). A projection's rows hold no lane.
pub(super) fn stack_at(t: &Transcript, out: &mut [RowSpec], at: std::ops::Range<usize>) {
    for i in at {
        if out.get(i).is_none_or(|r| kind(t, &r.content) != Kind::Divider) {
            continue;
        }
        let above = match i.checked_sub(1).map(|j| kind(t, &out[j].content)) {
            Some(Kind::Divider) => Beside::Divider,
            Some(Kind::Prompt) => Beside::Spaced,
            _ => Beside::Other,
        };
        let below = match out.get(i + 1).map(|r| kind(t, &r.content)) {
            Some(Kind::Divider) => Beside::Divider,
            Some(Kind::Prompt | Kind::Roomy) | None => Beside::Spaced,
            Some(Kind::Other) => Beside::Other,
        };
        let over = if i > 0 { Over::Above } else { Over::Own };
        match &mut out[i].content {
            RowContent::Notice(_, r, _) | RowContent::Compaction { run: r, .. } | RowContent::CompactionQueued(r) | RowContent::TurnDivider(r) => *r = Run { above, below, over },
            _ => {}
        }
        if i > 0 {
            out[i - 1].divider = Some(above);
        }
    }
}

/// `row` is the first row with messages still to load above it: a divider takes no room over
/// its words, since the row that lands above it brings that room (`Over::Waiting`).
pub(super) fn wait(row: &mut RowContent) {
    if let RowContent::Notice(_, r, _) | RowContent::Compaction { run: r, .. } | RowContent::CompactionQueued(r) | RowContent::TurnDivider(r) = row {
        r.over = Over::Waiting;
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Divider,
    /// A prompt: room over it, and its copy line under it.
    Prompt,
    /// Room over it only: a prompt's pictures, a note, Working.
    Roomy,
    Other,
}

fn kind(t: &Transcript, c: &RowContent) -> Kind {
    match c {
        RowContent::Notice(i, ..) => match &t.messages[*i] {
            Message::Notice(n) if crate::turn_errors::is_divider(n) => Kind::Divider,
            _ => Kind::Other,
        },
        RowContent::Compaction { message, piece, .. } => match &t.messages[*message] {
            Message::Compaction(_) if crate::compaction::is_divider(*piece) => Kind::Divider,
            _ => Kind::Other,
        },
        RowContent::CompactionQueued(_) | RowContent::TurnDivider(_) => Kind::Divider,
        RowContent::User { .. } | RowContent::Pending { .. } => Kind::Prompt,
        RowContent::UserImages { .. } | RowContent::Actions { .. } => Kind::Roomy,
        _ => Kind::Other,
    }
}

/// Each divider row's key and what sits beside it, top to bottom.
pub fn divider_runs<S: TimelineHost>(s: &S, t: &Transcript) -> Vec<(String, Run)> {
    super::rows(s, t).iter().filter(|r| kind(t, &r.content) == Kind::Divider).map(|r| {
        let run = match r.content {
            RowContent::Notice(_, run, _) | RowContent::Compaction { run, .. } | RowContent::CompactionQueued(run) | RowContent::TurnDivider(run) => run,
            _ => unreachable!(),
        };
        (r.key.clone(), run)
    }).collect()
}
