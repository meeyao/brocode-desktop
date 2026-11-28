//! The changes a script made to files (`session-ui/src/tools/tool-renderer.tsx`'s
//! `ScriptChanges`, `ScriptChangeGroup`, `anomalyco/opencode#65002`, `#65247`, `#66751`):
//! consecutive changes with a diff from the same script share one stacked patch group
//! ([`super::files::stacked_group`]), one row per path, merging repeated changes to the same path
//! under its single row and keying disclosure by the id of its first change. A change without a
//! diff (past the size cap, a binary file, counts alone) stands alone as a counts row
//! ([`super::files::counts_row`]).

use opencode_kit::model::ToolStatus;
use rune::{El, col};
use serde_json::{Value, json};

use super::{Call, Ctx};
use crate::TimelineHost;
use crate::palette::Pal;

/// The id of the change at offset `index` of script `script` in assistant message `msg`: its
/// card's call and disclosure key. Providers can reuse a call id across messages, so the message
/// is part of it. A key holds no `/`, so the path stays out of it.
pub fn change_id(msg: &str, script: &str, index: usize) -> String {
    format!("{msg}-{script}-file-{index}")
}

/// Whether change `file` draws as a card rather than a row of counts, as `changedFileDiff`
/// reads it: a patch with lines in it.
pub fn is_card(file: &Value) -> bool {
    use crate::activity::rules::count;
    file["patch"].is_string() && (count(&file["additions"]) > 0 || count(&file["deletions"]) > 0)
}

/// The one-file patch call's metadata change `file` draws through, built only when its card does.
fn payload(file: &Value) -> Value {
    use crate::activity::rules::count;
    let status = file["status"].as_str().unwrap_or("modified");
    json!({ "files": [{ "file": file["file"], "patch": file["patch"], "additions": count(&file["additions"]), "deletions": count(&file["deletions"]), "status": status }] })
}

/// Between a patch group and whatever is beside it, and between a script's changes and the files
/// edited beside them: `--timeline-gap-s` (`timeline-next.css`).
pub const CARD_GAP: f32 = 16.0;

/// Where each script's Output row goes under `full_codemode` (`scriptOutputLayout`,
/// `packages/session-ui/src/timeline/script-output.ts`): after its last change when its changes
/// draw right after it, else under its steps.
#[derive(Default)]
pub struct OutputPlacement<'a> {
    /// Script part IDs whose Output row moves after their changes.
    pub after_changes: std::collections::HashSet<&'a str>,
    /// Keyed by `change_id(msg, script, index)`: the script ID and trimmed output text to draw
    /// right after that change.
    pub output_after: std::collections::HashMap<String, (&'a str, String)>,
}

/// One run inside [`changes`] (`ScriptChangeRun`, `tool-renderer.tsx:1155-1170`, `opencode#66751`):
/// consecutive diff-bearing changes of the same script share one stacked patch group; each
/// non-diff change stands alone as a counts row.
enum Run {
    Patch(std::ops::Range<usize>),
    Counts(usize),
}

fn runs(
    list: &[(&str, &str, usize, &Value)],
    output_after: &std::collections::HashMap<String, (&str, String)>,
) -> Vec<Run> {
    let mut out: Vec<Run> = Vec::new();
    let mut i = 0;
    while i < list.len() {
        let (msg, script, index, file) = list[i];
        if !is_card(file) {
            out.push(Run::Counts(i));
            i += 1;
            continue;
        }
        let start = i;
        i += 1;
        // Keep grouping consecutive diff-bearing changes of the same script, stopping after any
        // change that places the script's Output row directly below it.
        let mut prev_id = change_id(msg, script, index);
        while i < list.len() && !output_after.contains_key(&prev_id) {
            let (m, s, idx, f) = list[i];
            if m != msg || s != script || !is_card(f) {
                break;
            }
            prev_id = change_id(m, s, idx);
            i += 1;
        }
        out.push(Run::Patch(start..i));
    }
    out
}

/// A script's changes, `(msg, script, index, file)` in order (`ScriptChanges`,
/// `tool-renderer.tsx:1178-1292`, `opencode#66751`). Consecutive diff-bearing changes from the
/// same script share one stacked patch group; non-diff changes render as flush counts rows;
/// a patch group stands [`CARD_GAP`] from whatever is beside it. When `output_after` names a
/// script whose last change ends a run, its flat `Output` row follows `CARD_GAP` below it.
pub fn changes<S: TimelineHost>(
    s: &S,
    p: &Pal,
    cx: Ctx,
    list: &[(&str, &str, usize, &Value)],
    output_after: &std::collections::HashMap<String, (&str, String)>,
) -> El {
    let mut out = col().key("script-changes").w_full();
    // Whether the item above drew a card/patch group; `None` above the first.
    let mut above: Option<bool> = None;
    for run in runs(list, output_after) {
        let (card, item, last_id) = match run {
            Run::Patch(range) => {
                let slice = &list[range];
                let ids: Vec<String> = slice.iter().map(|&(m, sc, idx, _)| change_id(m, sc, idx)).collect();
                let probes: Vec<Call> = slice
                    .iter()
                    .zip(&ids)
                    .map(|(&(_, _, _, file), id)| Call {
                        id,
                        name: "patch",
                        status: &ToolStatus::Completed,
                        input: &Value::Null,
                        metadata: file,
                    })
                    .collect();
                let drawn = super::cached(s, p, cx, &probes, || {
                    let payloads: Vec<Value> = slice.iter().map(|&(_, _, _, file)| payload(file)).collect();
                    let calls: Vec<Call> = probes
                        .iter()
                        .zip(&payloads)
                        .map(|(probe, metadata)| Call { metadata, ..*probe })
                        .collect();
                    super::files::stacked_group(s, p, cx, &calls)
                });
                let first_id = ids[0].clone();
                let last_id = ids.last().cloned().unwrap_or_default();
                (true, col().key(format!("change-{first_id}")).w_full().child(drawn), last_id)
            }
            Run::Counts(at) => {
                let (msg, script, index, file) = list[at];
                let id = change_id(msg, script, index);
                let row = super::files::counts_row(s, p, cx, format!("change-{id}"), file);
                (false, row, id)
            }
        };
        // Only two rows of counts in a row sit flush.
        let gap = above.is_some_and(|a| a || card);
        out = out.child(if gap { item.mt(CARD_GAP) } else { item });
        above = Some(card);
        if let Some((script_id, text)) = output_after.get(&last_id).filter(|(_, t)| !t.is_empty()) {
            out = out.child(super::execute::output_row(s, p, cx.session, script_id, text, true).mt(CARD_GAP));
            above = Some(true);
        }
    }
    out
}
