//! Naming a section in place: a new section opens its name field on a name its tabs' titles
//! share (selected, so typing replaces it), or empty under a quiet "Section name"; a rename opens
//! it on the name it has. ⏎ or a click elsewhere keeps what's typed, Esc keeps the name it had, an
//! empty field keeps it too.
//!
//! The name is guessed here, at once, from the tabs' titles alone: no server asks, and no
//! project's slug is ever offered as a name.

use rune::TextEdit;

/// The name being edited: arming until its field takes focus, then editing. `before` is the
/// name kept when the field is left empty or Esc is pressed; `created` marks a brand-new section
/// whose gathered tabs are flying in under its header.
#[derive(Default)]
pub enum Naming {
    #[default]
    Idle,
    Arming { section: String, field: TextEdit, before: String, created: bool },
    Editing { section: String, field: TextEdit, before: String, created: bool },
}

pub enum Event<'a> {
    /// Name `section`: the field opens on `text`, and `before` stays if nothing is typed.
    Start { section: &'a str, text: &'a str, before: &'a str, created: bool },
    /// The field's focus, read each frame while naming.
    Focused(bool),
    /// ⏎.
    Commit,
    /// Esc.
    Cancel,
    /// Section `id` went while it was named.
    Gone(&'a str),
}

#[derive(Debug, PartialEq)]
pub enum Effect {
    None,
    Focus,
    Save { section: String, name: String },
}

impl Naming {
    pub fn step(&mut self, event: Event<'_>) -> Effect {
        match (std::mem::take(self), event) {
            (_, Event::Start { section, text, before, created }) => {
                let mut field = TextEdit::new(text);
                field.set_selection(0, field.text.len());
                *self = Naming::Arming { section: section.to_owned(), field, before: before.to_owned(), created };
                Effect::Focus
            }
            (Naming::Arming { section, field, before, created } | Naming::Editing { section, field, before, created }, Event::Focused(true)) => {
                *self = Naming::Editing { section, field, before, created };
                Effect::None
            }
            (Naming::Arming { section, field, before, created }, Event::Focused(false)) => {
                *self = Naming::Arming { section, field, before, created };
                Effect::Focus
            }
            (Naming::Editing { section, field, before, .. }, Event::Focused(false))
            | (Naming::Arming { section, field, before, .. } | Naming::Editing { section, field, before, .. }, Event::Commit) => {
                let typed = field.text.trim();
                Effect::Save { section, name: if typed.is_empty() { before } else { typed.to_owned() } }
            }
            (Naming::Arming { section, before, .. } | Naming::Editing { section, before, .. }, Event::Cancel) => Effect::Save { section, name: before },
            (Naming::Arming { section, .. } | Naming::Editing { section, .. }, Event::Gone(id)) if section == id => Effect::None,
            (state, _) => {
                *self = state;
                Effect::None
            }
        }
    }

    /// The section being named and its field.
    pub fn editing(&self) -> Option<(&str, &TextEdit)> {
        match self {
            Naming::Arming { section, field, .. } | Naming::Editing { section, field, .. } => Some((section, field)),
            Naming::Idle => None,
        }
    }

    /// Whether `id` is a brand-new section currently in its initial naming pass.
    pub fn is_created(&self, id: &str) -> bool {
        matches!(self, Naming::Arming { section, created: true, .. } | Naming::Editing { section, created: true, .. } if section == id)
    }

    pub fn field_mut(&mut self) -> Option<&mut TextEdit> {
        match self {
            Naming::Arming { field, .. } | Naming::Editing { field, .. } => Some(field),
            Naming::Idle => None,
        }
    }

    /// Whether the field has focus, while naming.
    pub fn focused(&self) -> Option<bool> {
        self.editing().map(|(_, field)| field.focused)
    }
}

/// Words that name nothing on their own.
const STOP: &[&str] = &["a", "an", "and", "the", "of", "or", "to", "in", "on", "for", "with", "fix", "add", "new", "make", "update", "wip", "session"];

/// A title's words broken at spaces, marks and letter-digit seams, each as typed and as compared
/// (lowercase; a lone roman numeral as its number): "PHASE II: LEAD" and "Phase2 runner" both
/// start "phase", "2".
fn tokens(title: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for word in title.split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()) {
        let mut part = String::new();
        let mut digit = None;
        for c in word.chars() {
            let d = c.is_ascii_digit();
            if digit.is_some_and(|was| was != d) && !part.is_empty() {
                out.push(std::mem::take(&mut part));
            }
            digit = Some(d);
            part.push(c);
        }
        if !part.is_empty() {
            out.push(part);
        }
    }
    out.into_iter().map(|raw| (compare(&raw), raw)).collect()
}

fn compare(raw: &str) -> String {
    let lower = raw.to_lowercase();
    let roman = ["ii", "iii", "iv", "v", "vi", "vii", "viii", "ix", "x"];
    match roman.iter().position(|r| *r == lower) {
        // "v" and "x" alone are as often letters: only the longer numerals count.
        Some(i) if lower.len() > 1 => (i + 2).to_string(),
        _ => lower,
    }
}

/// A word as a section's name shows it: "INFERENCE" reads "Inference", "lumen" reads "Lumen".
fn show(raw: &str) -> String {
    let shouting = raw.chars().filter(|c| c.is_alphabetic()).count() > 1 && !raw.chars().any(char::is_lowercase);
    let word = if shouting { raw.to_lowercase() } else { raw.to_owned() };
    let mut c = word.chars();
    c.next().map(|f| f.to_uppercase().chain(c).collect()).unwrap_or_default()
}

fn stop(t: &str) -> bool {
    STOP.contains(&t)
}

/// A name for a section of tabs titled `titles`, only when two or more of them plainly share
/// one: the leading words every title starts with ("Inference" from "INFERENCE ROUTING" and
/// "INFERENCE STATS", "Phase 2"), or else a word every title has. Anything less is no guess.
pub fn suggest(titles: &[&str]) -> Option<String> {
    if titles.len() < 2 {
        return None;
    }
    let toks: Vec<Vec<(String, String)>> = titles.iter().map(|t| tokens(t)).collect();
    let first = toks.first()?;
    let shared = first.iter().take(3).enumerate().take_while(|(i, (t, _))| toks.iter().all(|w| w.get(*i).is_some_and(|(x, _)| x == t))).count();
    // "Fix login page" and "Fix login redirect" share "Login", not "Fix Login".
    let lead: Vec<&(String, String)> = first[..shared].iter().skip_while(|(t, _)| stop(t)).collect();
    if lead.iter().any(|(t, _)| t.chars().any(char::is_alphabetic)) {
        return Some(lead.iter().map(|(_, r)| show(r)).collect::<Vec<_>>().join(" "));
    }
    first
        .iter()
        .find(|(t, _)| !stop(t) && t.chars().filter(|c| c.is_alphabetic()).count() >= 3 && toks.iter().all(|w| w.iter().any(|(x, _)| x == t)))
        .map(|(_, raw)| show(raw))
}
