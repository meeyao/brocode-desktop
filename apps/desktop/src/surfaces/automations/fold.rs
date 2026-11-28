//! Folding long bodies in the inspector (a prompt, code, a run's output), as a long prompt folds
//! in the timeline (`session_view::fold`): one past 12 lines shows its first 8, the last of them
//! fading into a Read more / Show less toggle. The text is laid out whole and clipped, so copy and
//! select-all still get all of it, and nothing it holds can paint past its card.

use std::collections::BTreeSet;

use rune::TextStyle;

/// `session_view::fold`'s limits: past 12 lines a body shows 8, so a fold always hides 5 at least.
pub const OVER: usize = 12;
pub const SHOW: usize = 8;
/// The fade at the folded edge: three of the inspector's lines, so the text thins out over
/// them rather than its last line alone going grey.
pub const FADE: f32 = 60.0;

/// One body's fold.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fold {
    /// Its first [`SHOW`] lines.
    Folded,
    /// All of it.
    Open,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    /// The reader pressed the toggle.
    Toggle,
}

/// The bodies the reader opened, by key; one not listed is folded.
#[derive(Clone, Debug, Default)]
pub struct Folds(BTreeSet<String>);

impl Folds {
    pub fn get(&self, key: &str) -> Fold {
        if self.0.contains(key) { Fold::Open } else { Fold::Folded }
    }

    /// One transition of body `key`.
    pub fn step(&mut self, key: &str, event: Event) {
        match (self.get(key), event) {
            (Fold::Folded, Event::Toggle) => {
                self.0.insert(key.to_owned());
            }
            (Fold::Open, Event::Toggle) => {
                self.0.remove(key);
            }
        }
    }
}

/// How many lines `body` lays out to in `style` at `width`: its hard line breaks at least, and
/// its wrapped height when it could be long.
pub fn lines(body: &str, style: TextStyle, width: f32) -> usize {
    let hard = body.trim_end().split('\n').count();
    // At 12 pt a line holds 30 characters or more in any card; shorter text can't pass the limit.
    if hard > OVER || body.len() <= 30 * OVER {
        return hard;
    }
    let (_, h) = rune::text::measure_with(body, &[], style, Some(width.max(1.0)), rune::text::RichLayoutOptions::new().wrap_anywhere());
    hard.max((h / style.line_px()).round() as usize)
}
