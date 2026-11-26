//! ↑ and ↓ in the composer recall sent prompts (`p254`), matching OpenCode Desktop's
//! `composer/editor/interaction.ts` (`navigateHistory`).
//!
//! - Up to 200 prompts newest-first, kept apart for normal vs shell mode (`MAX_HISTORY`).
//! - ↑ starts only on an empty composer with the caret at 0. Once a recalled prompt is showing,
//!   ↑ and ↓ step while the caret sits at 0 or at the end; anywhere else the arrow moves the caret.
//! - ↑ places the caret at 0, ↓ at the end, and ↓ past the newest restores the earlier draft.
//! - Editing recalled text, switching scope/session, or toggling shell mode ends the walk.

use std::ops::Range;

use rune::{Key, KeyEvent, Mark, MarkRange, TextEdit};
use serde_json::{Value, json};

/// `MAX_HISTORY` (`packages/app/src/composer/history/entry.ts:12`).
const MAX: usize = 200;

/// A prompt as typed: its text and its `@` pills (byte range, id).
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Entry {
    pub text: String,
    pub pills: Vec<(Range<usize>, String)>,
}

impl Entry {
    pub fn of(edit: &TextEdit) -> Entry {
        Entry {
            text: edit.text.clone(),
            pills: edit.pills().map(|(r, id)| (r, id.to_owned())).collect(),
        }
    }

    pub fn put(&self, edit: &mut TextEdit, caret: usize) {
        edit.replace_text(self.text.clone());
        edit.set_marks(
            self.pills
                .iter()
                .map(|(r, id)| MarkRange::new(r.clone(), Mark::Pill(id.clone())))
                .collect(),
        );
        edit.set_caret(caret);
    }

    pub fn json(&self) -> Value {
        json!({
            "text": self.text,
            "pills": self.pills.iter().map(|(r, id)| json!([r.start, r.end, id])).collect::<Vec<_>>(),
        })
    }

    pub fn from_json(v: &Value) -> Option<Entry> {
        let text = v["text"].as_str()?.to_owned();
        let pills = v["pills"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|p| {
                let (s, e) = (p[0].as_u64()? as usize, p[1].as_u64()? as usize);
                (s < e && e <= text.len() && text.is_char_boundary(s) && text.is_char_boundary(e))
                    .then(|| (s..e, p[2].as_str().unwrap_or_default().to_owned()))
            })
            .collect();
        Some(Entry { text, pills })
    }
}

/// Where ↑ and ↓ are in the sent prompts, as Desktop's composer machine keeps it
/// (`composer/suggestions/machine.ts`, its `browsing` popover). A host without events of its
/// own sees the walk end when the shown text, the scope or the mode changed under it.
#[derive(Clone, Debug)]
enum Recall<Scope> {
    Idle,
    Browsing {
        index: usize,
        draft: Entry,
        caret: usize,
        shown: String,
        scope: Scope,
        shell: bool,
    },
}

/// Whether ↑ (`up`) or ↓ steps through history with the caret at `caret` in `len` bytes
/// (`canNavigateHistory`): while browsing, at either end; otherwise ↑ only on an empty draft,
/// and ↓ at its end.
pub fn can_step(up: bool, len: usize, caret: usize, browsing: bool) -> bool {
    let at = caret.min(len);
    if browsing {
        return at == 0 || at == len;
    }
    if up { at == 0 && len == 0 } else { at == len }
}

/// Shared prompt-recall store parameterized by page/session scope (`String` by default).
#[derive(Clone, Debug)]
pub struct PromptHistory<Scope = String> {
    normal: Vec<Entry>,
    shell: Vec<Entry>,
    recall: Recall<Scope>,
}

impl<Scope> Default for PromptHistory<Scope> {
    fn default() -> Self {
        Self {
            normal: Vec::new(),
            shell: Vec::new(),
            recall: Recall::Idle,
        }
    }
}

impl<Scope: Clone + PartialEq> PromptHistory<Scope> {
    pub fn from_json(v: &Value) -> Self {
        let list = |k: &str| {
            v[k].as_array()
                .into_iter()
                .flatten()
                .filter_map(Entry::from_json)
                .take(MAX)
                .collect()
        };
        Self {
            normal: list("normal"),
            shell: list("shell"),
            recall: Recall::Idle,
        }
    }

    pub fn to_json(&self) -> Value {
        json!({
            "normal": self.normal.iter().map(Entry::json).collect::<Vec<_>>(),
            "shell": self.shell.iter().map(Entry::json).collect::<Vec<_>>(),
        })
    }

    pub fn entries(&self, shell: bool) -> &[Entry] {
        if shell { &self.shell } else { &self.normal }
    }

    /// Records a sent prompt and ends any active walk. Returns `true` when the stored list changed.
    pub fn add(&mut self, entry: Entry, shell: bool) -> bool {
        self.recall = Recall::Idle;
        if entry.text.trim().is_empty() {
            return false;
        }
        let list = if shell { &mut self.shell } else { &mut self.normal };
        if list.first() == Some(&entry) {
            return false;
        }
        list.insert(0, entry);
        list.truncate(MAX);
        true
    }

    /// Removes entries matching `entry` (`removeHistoryEntry`, `entry.ts:67-76`), used when a
    /// failed send restores its draft back into the composer. Returns `true` if the list changed.
    pub fn remove(&mut self, entry: &Entry, shell: bool) -> bool {
        let list = if shell { &mut self.shell } else { &mut self.normal };
        let before = list.len();
        list.retain(|item| item != entry);
        let changed = list.len() != before;
        if changed {
            self.recall = Recall::Idle;
        }
        changed
    }

    /// ↑ or ↓ on the composer: `true` when it recalled or restored a prompt. Brocode
    /// runs this walk in its composer machine; this is for hosts with no machine of their own.
    pub fn step(&mut self, composer: &mut TextEdit, scope: &Scope, shell: bool, up: bool) -> bool {
        if composer.anchor != composer.focus || composer.is_composing() {
            return false;
        }
        if let Recall::Browsing { shown, scope: s, shell: sh, .. } = &self.recall
            && (*shown != composer.text || *sh != shell || s != scope)
        {
            self.recall = Recall::Idle;
        }
        let at = composer.focus.min(composer.text.len());
        let browsing = match &self.recall {
            Recall::Browsing { index, .. } => Some(*index),
            Recall::Idle => None,
        };
        if !can_step(up, composer.text.len(), at, browsing.is_some()) {
            return false;
        }
        let list = if shell { &self.shell } else { &self.normal };
        if up {
            let index = browsing.map_or(0, |i| i + 1);
            let Some(entry) = list.get(index) else { return false };
            let (draft, caret) = match std::mem::replace(&mut self.recall, Recall::Idle) {
                Recall::Browsing { draft, caret, .. } => (draft, caret),
                Recall::Idle => (Entry::of(composer), at),
            };
            entry.put(composer, 0);
            self.recall = Recall::Browsing { index, draft, caret, shown: composer.text.clone(), scope: scope.clone(), shell };
            return true;
        }
        let Recall::Browsing { index, draft, caret, .. } = std::mem::replace(&mut self.recall, Recall::Idle) else { return false };
        match index.checked_sub(1) {
            Some(newer) => {
                let entry = &list[newer];
                entry.put(composer, entry.text.len());
                self.recall = Recall::Browsing { index: newer, draft, caret, shown: composer.text.clone(), scope: scope.clone(), shell };
            }
            None => draft.put(composer, caret),
        }
        true
    }
}

/// Unmodified ↑ (`Some(true)`) or ↓ (`Some(false)`).
pub fn arrow_up(ev: &KeyEvent) -> Option<bool> {
    if ev.mods.alt || ev.mods.ctrl || ev.mods.cmd || ev.mods.shift {
        return None;
    }
    match ev.key {
        Key::Up => Some(true),
        Key::Down => Some(false),
        _ => None,
    }
}

