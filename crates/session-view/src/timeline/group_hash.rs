//! Cached incremental memo-key hashing for collapsed assistant turn groups (`c-*`).
//!
//! A single long turn can span ~100 assistant steps and hundreds of `execute` tool calls.
//! Re-hashing every message's `serde_json::Value` metadata on every `RowSpec::build` costs
//! several milliseconds per rebuild even when `rune::memo` hits. Keeping per-message hashes
//! keyed on the host's transcript version (and narrowing invalidation via
//! `TimelineHost::transcript_changed_from`) makes unchanged groups O(1).

use std::cell::RefCell;
use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::ops::Range;

use opencode_kit::model::{Message, Part, Transcript};

use crate::TimelineHost;

/// Sessions kept in the thread-local cache (matches `projection::KEPT`).
const KEPT: usize = 4;

struct SessionCache {
    version: u64,
    edits: bool,
    script_diffs: bool,
    len: usize,
    first_id: Option<String>,
    last_id: Option<String>,
    per_msg: Vec<Option<u64>>,
    /// Cached range hash keyed by `range.start -> (range.end, hash)` so growing turns replace
    /// their previous prefix entry instead of accumulating stale `(start, end)` keys.
    ranges: HashMap<usize, (usize, u64)>,
}

impl SessionCache {
    fn new(version: u64, edits: bool, script_diffs: bool, t: &Transcript) -> Self {
        Self {
            version,
            edits,
            script_diffs,
            len: t.messages.len(),
            first_id: t.messages.first().map(|m| m.id().to_owned()),
            last_id: t.messages.last().map(|m| m.id().to_owned()),
            per_msg: vec![None; t.messages.len()],
            ranges: HashMap::new(),
        }
    }

    fn reset(&mut self, version: u64, edits: bool, script_diffs: bool, t: &Transcript) {
        self.version = version;
        self.edits = edits;
        self.script_diffs = script_diffs;
        self.len = t.messages.len();
        self.first_id = t.messages.first().map(|m| m.id().to_owned());
        self.last_id = t.messages.last().map(|m| m.id().to_owned());
        self.per_msg.clear();
        self.per_msg.resize(t.messages.len(), None);
        self.ranges.clear();
    }

    fn same_shape(&self, t: &Transcript) -> bool {
        self.len == t.messages.len()
            && self.first_id.as_deref() == t.messages.first().map(Message::id)
            && self.last_id.as_deref() == t.messages.last().map(Message::id)
    }

    fn get_range(&self, range: &Range<usize>) -> Option<u64> {
        self.ranges
            .get(&range.start)
            .filter(|&&(end, _)| end == range.end)
            .map(|&(_, v)| v)
    }
}

thread_local! {
    static CACHE: RefCell<HashMap<String, SessionCache>> = RefCell::new(HashMap::new());
}

/// Hash of the turn slice `range` of `t`, cached per session and incrementally updated when only
/// the tail of `t` changed.
pub(super) fn of<S: TimelineHost>(
    s: &S,
    t: &Transcript,
    range: Range<usize>,
    detail: &crate::detail::Detail,
    script_diffs: bool,
) -> u64 {
    let edits = script_diffs && detail.placement(crate::detail::Category::Edit) != crate::detail::Placement::Hidden;
    let Some(version) = s.transcript_version(&t.session) else {
        return hash_uncached(&t.messages[range], edits, script_diffs);
    };
    CACHE.with(|cell| {
        let mut map = cell.borrow_mut();
        if !map.contains_key(&t.session) && map.len() >= KEPT {
            map.clear();
        }
        let entry = match map.entry(t.session.clone()) {
            Entry::Occupied(occ) => {
                let entry = occ.into_mut();
                let same_flags = entry.edits == edits && entry.script_diffs == script_diffs;
                if same_flags && entry.version == version && entry.same_shape(t) {
                    if let Some(v) = entry.get_range(&range) {
                        return v;
                    }
                } else if same_flags {
                    match s.transcript_changed_from(&t.session, entry.version) {
                        Some(usize::MAX) if entry.same_shape(t) => {
                            // Only another session (e.g. a running subagent) streamed between versions.
                            entry.version = version;
                            if let Some(v) = entry.get_range(&range) {
                                return v;
                            }
                        }
                        Some(usize::MAX) => entry.reset(version, edits, script_diffs, t),
                        Some(at) if at == 0 || entry.first_id.as_deref() == t.messages.first().map(Message::id) => {
                            entry.per_msg.truncate(at.min(entry.per_msg.len()));
                            entry.per_msg.resize(t.messages.len(), None);
                            entry.ranges.retain(|_, &mut (end, _)| end <= at);
                            entry.version = version;
                            entry.len = t.messages.len();
                            entry.first_id = t.messages.first().map(|m| m.id().to_owned());
                            entry.last_id = t.messages.last().map(|m| m.id().to_owned());
                            if let Some(v) = entry.get_range(&range) {
                                return v;
                            }
                        }
                        _ => entry.reset(version, edits, script_diffs, t),
                    }
                } else {
                    entry.reset(version, edits, script_diffs, t);
                }
                entry
            }
            Entry::Vacant(vac) => vac.insert(SessionCache::new(version, edits, script_diffs, t)),
        };

        let mut h = DefaultHasher::new();
        (range.end - range.start).hash(&mut h);
        for (slot, m) in entry.per_msg[range.clone()].iter_mut().zip(&t.messages[range.clone()]) {
            let mh = *slot.get_or_insert_with(|| message_hash(m, edits, script_diffs));
            mh.hash(&mut h);
        }
        let combined = h.finish();
        entry.ranges.insert(range.start, (range.end, combined));
        combined
    })
}

fn hash_uncached(messages: &[Message], edits: bool, script_diffs: bool) -> u64 {
    let mut h = DefaultHasher::new();
    messages.len().hash(&mut h);
    for m in messages {
        message_hash(m, edits, script_diffs).hash(&mut h);
    }
    h.finish()
}

fn message_hash(m: &Message, edits: bool, script_diffs: bool) -> u64 {
    let mut h = DefaultHasher::new();
    if let Message::Notice(n) = m {
        (3u8, &n.id, format!("{:?}", n.kind)).hash(&mut h);
    }
    let step = match m {
        Message::Assistant(a) => Some(a),
        Message::Note { step, .. } => Some(&**step),
        _ => None,
    };
    if let Some(a) = step {
        (&a.id, a.completed, a.parts.len(), &a.text_phases).hash(&mut h);
        for p in &a.parts {
            match p {
                Part::Tool { id, name, title, status, input, files, metadata } => {
                    let call = crate::tools::Call { id, name, status, input, metadata };
                    let empty_input = input.as_object().is_none_or(|o| o.is_empty());
                    let out = opencode_kit::model::tool_output(status, metadata).unwrap_or("");
                    let ob = out.as_bytes();
                    (
                        0u8,
                        id,
                        name,
                        title,
                        std::mem::discriminant(status),
                        empty_input,
                        crate::tools::path(input),
                        files.len(),
                        crate::tools::file_draws_card(&call),
                        crate::tools::skill_name(&call),
                    )
                        .hash(&mut h);
                    (
                        input["command"].as_str(),
                        input["background"].as_bool(),
                        metadata["shellID"].as_str(),
                        metadata["error"].as_bool(),
                        opencode_kit::model::tool_error(metadata).map(str::len),
                        ob.len(),
                        &ob[..ob.len().min(16)],
                        &ob[ob.len().saturating_sub(16)..],
                        metadata["status"].as_str(),
                        metadata["exit"].as_i64(),
                        metadata["loaded"].as_array().map(Vec::len),
                        metadata["files"].as_array().map(Vec::len),
                    )
                        .hash(&mut h);
                    // A script's changes name the group as they arrive; progress that adds
                    // none keeps the row.
                    if edits {
                        crate::activity::rules::script_changes(name, status, metadata)
                            .for_each(|(i, f)| (i, f["file"].as_str(), crate::tools::script::is_card(f)).hash(&mut h));
                    }
                    if name == "execute" {
                        if script_diffs {
                            metadata["toolCalls"].as_array().into_iter().flatten().for_each(|c| {
                                (
                                    c["tool"].as_str(),
                                    c["status"].as_str(),
                                    c["input"]["method"].as_str(),
                                    c["input"]["filePath"].as_str(),
                                    c["input"]["path"].as_str(),
                                )
                                    .hash(&mut h);
                            });
                        } else {
                            crate::tools::execute_has_calls(metadata).hash(&mut h);
                        }
                    }
                }
                Part::Reasoning { text, completed, .. } => (1u8, text.len(), completed).hash(&mut h),
                Part::Text { text } => (2u8, text.len()).hash(&mut h),
            }
        }
    }
    h.finish()
}
