//! The question dock: Desktop's `session-question-dock.tsx` and `question-keys.ts`. When the
//! open session waits on a question, the composer's surface morphs into it (`ComposerMorph`,
//! `spring-height.tsx`): one card whose height springs on Desktop's `docks.morph` (0.4 s, no
//! bounce) while the composer's face fades out and the question's fades up. The curves and
//! timings are knobs (⌘⇧K → Question dock, `dock::knobs`) whose defaults are Desktop's.
//! Answering or dismissing it, here or anywhere else, morphs it back with the draft as it was.
//!
//! Keys: ↑/↓ (Home/End) move the highlight, 1–9 pick, Space toggles, ↵ picks the highlighted
//! answer and moves on (submitting on the last question), ←/→ and ⌘[ change question, ⌘↵ goes
//! on or submits, Esc dismisses. A printable key on "Type your own answer" starts typing it.
//! In the field, keys type (digits too), Esc leaves it, and ↑ on its first line goes up to the
//! answer above. Submitting with a required question unanswered goes to that question.
//!
//! A long question keeps to part of the pane (p273): the dock is at most ⌘⇧K → Question dock →
//! Max height of the pane it sits in, so the timeline still shows above it.

use std::collections::{HashMap, HashSet};

use opencode_kit::model::{Field, FieldKind, Pending};
use rune::{Key, KeyEvent, TextEdit, Tx};
use serde_json::Value;

use super::{DockHost, knobs, morph};

mod classic;
mod composer;

/// Whether the dock is Desktop's Classic one rather than the composer-integrated New question UI
/// (`general.questionComposer`, `opencode#66825`).
pub fn classic<S: DockHost>(s: &S) -> bool {
    !s.dock_question_composer()
}

/// The question's face, Classic or composer-integrated (`opencode#66825`).
pub fn face<S: DockHost>(s: &S, p: &crate::palette::Pal) -> Option<Vec<rune::El>> {
    if classic(s) { classic::face(s, p) } else { composer::face(s, p) }
}

/// Where the dock is for the form it shows: the question, the highlighted answer, and the
/// picked and typed answers in progress by `form/key`.
#[derive(Default)]
pub struct Dock {
    form: String,
    pub tab: usize,
    pub focus: usize,
    pub picks: HashMap<String, Vec<String>>,
    pub typed: HashMap<String, TextEdit>,
    /// `form/key`s whose typed answer counts.
    custom: HashSet<String>,
    /// The Classic dock folded down to its question (the header's chevron).
    pub minimized: bool,
    /// The dock's last input was a key, so the highlighted answer shows its focus ring.
    keyboard: bool,
    /// Highlighted preset option index in composer mode (`activeOption`, `opencode#66825`).
    active_option: Option<usize>,
}

impl Dock {
    /// Whether the typed answer at `form/key` counts towards the reply.
    pub fn custom_on(&self, key: &str) -> bool {
        self.custom.contains(key)
    }

    pub fn forget(&mut self, form: &str) {
        let prefix = format!("{form}/");
        self.picks.retain(|k, _| !k.starts_with(&prefix));
        self.typed.retain(|k, _| !k.starts_with(&prefix));
        self.custom.retain(|k| !k.starts_with(&prefix));
    }

    pub fn typed_mut(&mut self, form: &str, key: &str) -> &mut TextEdit {
        self.typed.entry(format!("{form}/{key}")).or_insert_with(|| TextEdit::new(""))
    }

    /// Form `id`'s answer to send, or None while a required field is empty.
    pub fn answer(&self, id: &str, fields: &[Field]) -> Option<Value> {
        let mut answer = serde_json::Map::new();
        for f in fields {
            let k = format!("{id}/{}", f.key);
            let typed = self.typed.get(&k).filter(|_| self.custom_on(&k)).map(|t| t.text.trim().to_owned()).filter(|t| !t.is_empty());
            let picks = self.picks.get(&k).cloned().unwrap_or_default();
            let v = match f.kind {
                FieldKind::Multi => {
                    let mut all = picks;
                    all.extend(typed);
                    (!all.is_empty()).then(|| Value::from(all))
                }
                FieldKind::Boolean => picks.first().map(|v| Value::Bool(v == "true")),
                FieldKind::Number => typed.and_then(|t| t.parse::<f64>().ok()).map(Value::from),
                FieldKind::Text => typed.or_else(|| picks.first().cloned()).map(Value::from),
            };
            match v {
                Some(v) => {
                    answer.insert(f.key.clone(), v);
                }
                None if f.required || fields.len() == 1 => return None,
                None => {}
            }
        }
        Some(Value::Object(answer))
    }
}

/// Whether keyboard focus sits inside the question dock (or nowhere specific, as right after the
/// dock opens). Unlike official (where `onKeyDown` is scoped to the dock DOM node), Rune's
/// `question_key` runs window-wide (`keys.rs:27`), so focus on widgets outside the dock (tabs,
/// sidebar, timeline) must not have `Left`/`Right`/`1..9`/`Escape`/`Cmd+Enter` hijacked.
pub(super) fn focus_in_dock(focused: Option<&str>) -> bool {
    match focused {
        None => true,
        Some(k) => k.starts_with("q-") || k.starts_with("question"),
    }
}

/// Whether keyboard focus `focused` sits on a question option, the composer-mode options listbox,
/// or nowhere specific (as opposed to a header/footer button like Dismiss, Back, Next/Submit,
/// Minimize, or progress segments).
pub(super) fn focus_in_options(focused: Option<&str>) -> bool {
    match focused {
        None | Some("q-custom" | "q-custom-mark" | "q-input") => true,
        Some(k) => k.starts_with("q-opt-") || k.starts_with("q-options-"),
    }
}

/// The element key that takes keyboard focus when the question dock opens (`land()` in
/// `session-question-dock-classic.tsx:299-305`): the custom answer field when there are no preset
/// options (or a typed answer is active in composer mode), the options listbox in composer mode,
/// or the first option in Classic mode.
pub fn initial_focus<S: DockHost>(s: &S) -> String {
    let Some(f) = asking(s) else { return "q-opt-0".to_owned() };
    let (tab, _) = place(s, &f);
    let n = rows(&f.fields[tab]).len();
    if classic(s) {
        if n > 0 { "q-opt-0".to_owned() } else { "q-input".to_owned() }
    } else {
        let k = f.key(tab);
        let has_typed = s.dock().custom_on(&k) && s.dock().typed.get(&k).is_some_and(|t| !t.text.is_empty());
        if n == 0 || has_typed { "q-input".to_owned() } else { format!("q-options-{tab}") }
    }
}

/// The answer index a focused option key names, so Tab + Enter acts on the focused row.
pub(super) fn focused_option(focused: Option<&str>, n: usize, has_custom: bool) -> Option<usize> {
    match focused {
        Some("q-custom" | "q-custom-mark") if has_custom => Some(n),
        Some(k) => k.strip_prefix("q-opt-").and_then(|s| s.parse::<usize>().ok()).filter(|&i| i < n),
        None => None,
    }
}

/// The typed answer's line (`question-custom-input`: `line-height: 18px`).
pub(super) const FIELD_LINE: f32 = 18.0;
/// The typed answer grows to six lines, then scrolls inside itself (`question-prompt.css:398-409`
/// and `question-prompt-experimental.css`: `max-block-size: 118px` with 5 over and under, and
/// `overflow-y: auto`).
pub(super) const FIELD_MAX_H: f32 = 6.0 * FIELD_LINE;

/// The key that moves the dock on, or submits it on the last question: Enter, or ⌘↵ / Ctrl+↵
/// (`QuestionSubmitKey`, `question-keys.ts:24`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SubmitKey {
    #[default]
    Enter,
    ModEnter,
}

/// What an Enter does in the dock: `Submit` moves on; `Select` picks the highlighted answer, or
/// in the typed answer starts a new line.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Enter {
    Submit,
    Select,
}

/// Desktop's `questionEnter` (`question-keys.ts:29-37`): ⌘↵ / Ctrl+↵ always submits. With Enter
/// submitting, Shift+Enter selects; with ⌘↵ submitting, Enter selects with or without Shift.
pub fn enter(ev: &KeyEvent, submit: SubmitKey) -> Option<Enter> {
    let m = ev.mods;
    if ev.key != Key::Enter || m.alt {
        return None;
    }
    if m.cmd || m.ctrl {
        return Some(Enter::Submit);
    }
    if submit == SubmitKey::ModEnter {
        return Some(Enter::Select);
    }
    Some(if m.shift { Enter::Select } else { Enter::Submit })
}

/// How the Next and Submit buttons show the submit key (`questionSubmitShortcuts`,
/// `question-submit.ts:4-8`): ⏎ alone, or ⌘⏎ on a Mac and Ctrl+⏎ elsewhere.
pub(super) fn submit_hint(submit: SubmitKey) -> String {
    match submit {
        SubmitKey::Enter => "⏎".to_owned(),
        SubmitKey::ModEnter if cfg!(target_os = "macos") => "⌘⏎".to_owned(),
        SubmitKey::ModEnter => format!("{}+⏎", opencode_i18n::t!("common.key.ctrl")),
    }
}

/// One answer's value, label and description. A yes/no question's answers are Yes and No.
pub fn rows(f: &Field) -> Vec<(String, String, String)> {
    if f.kind == FieldKind::Boolean {
        return vec![("true".into(), opencode_i18n::t!("provider.connect.form.yes").into(), String::new()), ("false".into(), opencode_i18n::t!("provider.connect.form.no").into(), String::new())];
    }
    f.options.iter().map(|o| (o.value.clone(), if o.label.is_empty() { o.value.clone() } else { o.label.clone() }, o.description.clone())).collect()
}

pub fn has_custom(f: &Field) -> bool {
    f.kind != FieldKind::Boolean && (f.custom || f.options.is_empty())
}

fn count(f: &Field) -> usize {
    rows(f).len() + has_custom(f) as usize
}

/// The question form the open session waits on.
pub struct Form {
    pub id: String,
    pub fields: Vec<Field>,
}

impl Form {
    fn key(&self, tab: usize) -> String {
        format!("{}/{}", self.id, self.fields[tab].key)
    }
    fn multi(&self, tab: usize) -> bool {
        self.fields[tab].kind == FieldKind::Multi
    }
    fn last(&self, tab: usize) -> bool {
        tab + 1 >= self.fields.len()
    }
}

/// The form the open page waits on. A subagent's page has no composer to stand in for.
pub fn asking<S: DockHost>(s: &S) -> Option<Form> {
    let id = s.active_session_id()?;
    if s.dock_parent(id).is_some() {
        return None;
    }
    s.dock_pending(id).into_iter().find_map(|p| match p {
        Pending::Form { id, fields, .. } if !fields.is_empty() => Some(Form { id, fields }),
        _ => None,
    })
}

/// The form the question dock shows.
pub fn id<S: DockHost>(s: &S) -> Option<String> {
    asking(s).map(|f| f.id)
}

/// The question and highlighted answer for `form`, as the dock left them or fresh.
fn place<S: DockHost>(s: &S, f: &Form) -> (usize, usize) {
    let d = s.dock();
    if d.form == f.id { (d.tab.min(f.fields.len() - 1), d.focus) } else { (0, pick_focus(s, f, 0)) }
}

fn dock<'a, S: DockHost>(s: &'a mut S, f: &Form) -> &'a mut Dock {
    if s.dock().form != f.id {
        let focus = pick_focus(s, f, 0);
        let d = s.dock_mut();
        (d.form, d.tab, d.focus) = (f.id.clone(), 0, focus);
        (d.minimized, d.keyboard, d.active_option) = (false, false, Some(focus));
    }
    s.dock_mut()
}

/// The answer a question opens on: its typed one when that's on, else its first pick.
fn pick_focus<S: DockHost>(s: &S, f: &Form, tab: usize) -> usize {
    let k = f.key(tab);
    let list = rows(&f.fields[tab]);
    if s.dock().custom_on(&k) && has_custom(&f.fields[tab]) {
        return list.len();
    }
    let picks = s.dock().picks.get(&k);
    list.iter().position(|(v, ..)| picks.is_some_and(|p| p.contains(v))).unwrap_or(0)
}

/// Whether the form's reply is on its way or taken (`isQuestionBusy`): the dock takes no answers.
fn busy<S: DockHost>(s: &S, f: &Form) -> bool {
    s.dock_busy(&f.id)
}

fn editing<S: DockHost>(s: &S, f: &Form, tab: usize) -> bool {
    s.dock().typed.get(&f.key(tab)).is_some_and(|t| t.focused)
}

/// Whether the dock takes letters now: its typed answer field has focus.
pub fn takes_text<S: DockHost>(s: &S) -> bool {
    let Some(f) = asking(s) else { return false };
    let (tab, _) = place(s, &f);
    editing(s, &f, tab)
}

/// Opens the current question's typed answer with `text` in it (the storybook's typing stories).
pub fn type_text<S: DockHost>(s: &mut S, text: &str) {
    let Some(f) = asking(s) else { return };
    let (tab, _) = place(s, &f);
    start_typing(s, &f, tab);
    let t = s.dock_mut().typed_mut(&f.id, &f.fields[tab].key);
    *t = TextEdit::new(text);
    t.focused = true;
}

/// Goes to the last question (the storybook's Custom answer story).
pub fn go_last<S: DockHost>(s: &mut S) {
    let Some(f) = asking(s) else { return };
    let last = f.fields.len() - 1;
    let focus = pick_focus(s, &f, last);
    let d = dock(s, &f);
    d.tab = last;
    d.focus = focus;
}

/// Leaves the typed answer's field as Esc does (the storybook's Custom answer story).
pub fn leave_field<S: DockHost>(s: &mut S) {
    let Some(f) = asking(s) else { return };
    let (tab, _) = place(s, &f);
    stop_typing(s, &f, tab);
    dock(s, &f).focus = rows(&f.fields[tab]).len();
}

fn answered<S: DockHost>(s: &S, f: &Form, tab: usize) -> bool {
    let k = f.key(tab);
    let d = s.dock();
    d.picks.get(&k).is_some_and(|p| !p.is_empty()) || (d.custom_on(&k) && d.typed.get(&k).is_some_and(|t| !t.text.trim().is_empty()))
}

fn stop_typing<S: DockHost>(s: &mut S, f: &Form, tab: usize) {
    if let Some(t) = s.dock_mut().typed.get_mut(&f.key(tab)) {
        t.focused = false;
    }
}

/// Picks answer `i`: the one answer of a single choice, or one more (or one fewer) of many.
fn select<S: DockHost>(s: &mut S, f: &Form, tab: usize, i: usize) {
    if busy(s, f) {
        return;
    }
    let Some((value, ..)) = rows(&f.fields[tab]).into_iter().nth(i) else { return };
    let k = f.key(tab);
    let clear_typed = !classic(s) && !f.multi(tab);
    let d = s.dock_mut();
    let picks = d.picks.entry(k.clone()).or_default();
    if f.multi(tab) {
        match picks.iter().position(|v| *v == value) {
            Some(at) => {
                picks.remove(at);
            }
            None => picks.push(value),
        }
    } else {
        *picks = vec![value];
        d.custom.remove(&k);
        if clear_typed {
            d.typed.remove(&k);
        }
    }
    stop_typing(s, f, tab);
}

fn start_typing<S: DockHost>(s: &mut S, f: &Form, tab: usize) {
    if busy(s, f) {
        return;
    }
    let k = f.key(tab);
    dock(s, f).focus = rows(&f.fields[tab]).len();
    let d = s.dock_mut();
    d.custom.insert(k.clone());
    if !f.multi(tab) {
        d.picks.remove(&k);
    }
    s.composer_edit_mut().focused = false;
    s.dock_mut().typed_mut(&f.id, &f.fields[tab].key).focused = true;
}

fn toggle_custom<S: DockHost>(s: &mut S, f: &Form, tab: usize) {
    if busy(s, f) {
        return;
    }
    let k = f.key(tab);
    if !f.multi(tab) || !s.dock().custom_on(&k) {
        return start_typing(s, f, tab);
    }
    s.dock_mut().custom.remove(&k);
    stop_typing(s, f, tab);
    dock(s, f).focus = rows(&f.fields[tab]).len();
}

fn jump<S: DockHost>(s: &mut S, f: &Form, tab: usize, tx: &mut Tx) {
    let (from, _) = place(s, f);
    if tab == from || tab >= f.fields.len() || busy(s, f) {
        return;
    }
    stop_typing(s, f, from);
    let focus = pick_focus(s, f, tab);
    let d = dock(s, f);
    d.tab = tab;
    d.focus = focus;
    d.active_option = Some(focus);
    tx.motion_curve(knobs::morph(s.knobs()));
    if !classic(s) {
        composer::land(s, f, tab, tx);
    }
}

/// Next question, or the reply on the last one.
fn next<S: DockHost>(s: &mut S, f: &Form, tx: &mut Tx) {
    let (tab, _) = place(s, f);
    stop_typing(s, f, tab);
    if !f.last(tab) {
        return jump(s, f, tab + 1, tx);
    }
    // Unanswered questions that may be skipped are left out, as Desktop does; a required one is
    // where the reply goes next instead.
    if s.dock().answer(&f.id, &f.fields).is_none() {
        let gap = (0..f.fields.len()).find(|&t| !answered(s, f, t) && (f.fields[t].required || f.fields.len() == 1));
        if let Some(t) = gap {
            jump(s, f, t, tx);
        }
        return;
    }
    s.dock_submit_form(&f.id);
    closed_if_gone(s, f, tx);
}

fn dismiss<S: DockHost>(s: &mut S, f: &Form, tx: &mut Tx) {
    s.dock_dismiss_form(&f.id);
    closed_if_gone(s, f, tx);
}

fn closed_if_gone<S: DockHost>(s: &mut S, f: &Form, tx: &mut Tx) {
    if !s.dock_waiting(&f.id) {
        morph(s, tx);
    }
}

/// A key while the dock shows; false when it isn't the dock's.
pub fn key<S: DockHost>(s: &mut S, ev: &KeyEvent, tx: &mut Tx) -> bool {
    if classic(s) {
        classic::key(s, ev, tx)
    } else {
        composer::key(s, ev, tx)
    }
}

/// Inserts a newline into the typed answer's field when plain Enter is the non-submitting Enter
/// (`SubmitKey::ModEnter`): in Rune a multi-line `text_input` inserts `\n` natively only on
/// Shift+Return, so plain Enter goes through here (`rune::el::input`).
pub(super) fn insert_typed_newline<S: DockHost>(s: &mut S, f: &Form, tab: usize) {
    let k = f.key(tab);
    let d = s.dock_mut();
    let _ = d.typed.entry(k.clone()).or_insert_with(|| TextEdit::new("")).insert("\n");
    d.custom.insert(k.clone());
    if !f.multi(tab) {
        d.picks.remove(&k);
    }
}

/// Brings the highlighted answer, or "Type your own answer", into sight; on the first answer as
/// much of the question above it as fits shows too (reveals run in order, each from where the
/// last left the list).
pub fn reveal_focus<S: DockHost>(s: &S, tx: &mut Tx) {
    let Some(f) = asking(s) else { return };
    let (tab, focus) = place(s, &f);
    let field = &f.fields[tab];
    let scope = format!("q-scroll-{tab}");
    if focus == 0 {
        tx.scroll_into_view_in(scope.clone(), format!("q-details-{tab}"));
    }
    let key = if focus >= rows(field).len() && has_custom(field) { "q-custom".to_owned() } else { format!("q-opt-{focus}") };
    tx.focus(key.clone(), true);
    tx.scroll_into_view_in(scope, key);
}
