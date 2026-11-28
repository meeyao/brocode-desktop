//! Reusable popup/context menus and pickers for Rune apps: native AppKit menus on macOS
//! (`Tx::popup_menu`) and themed, keyboard-accessible in-window menus (`menu-v2` and
//! `appearance="standard"`) everywhere else, built on [`crate::popover::MENU`].
//!
//! Pure menu data ([`Menu`], [`Item`]) is non-generic so feature crates can construct menus without
//! depending on host app state. Runtime state ([`Popup<S>`]), dispatch ([`open`], [`open_drawn`],
//! [`open_searchable`], [`dismiss`], [`poll`], [`key`]), trigger wiring ([`MenuButton`]), and
//! rendering ([`view`], [`surface`]) are parameterized over any host implementing [`PopupHost`].

use std::cell::RefCell;
use std::rc::Rc;

use rune::{El, TextEdit, TextStyle, Tx};
use rune_knobs::Knobs;
use crate::palette::Pal;
use crate::popover::{self, MENU};

pub mod focus;
mod keys;
mod menu;
pub mod native;
mod rows;
mod search;
mod standard;
mod state;
mod submenu;
pub mod trigger;
mod view;

pub use keys::key;
pub use menu::{Indicator, Item, Menu};
pub use rune::{Align, PopupAnchor, PopupItem, PopupMenu, Side};
pub use state::{dismiss, open, open_drawn, open_searchable, open_with_field, poll, set_highlight};
pub use trigger::{MenuButton, menu_button};
pub use view::{surface, view};

/// Native menus where Kit chose them (macOS).
pub const NATIVE: bool = cfg!(target_os = "macos");

/// Menu buttons and scrims act as the mouse button goes down on every desktop and web platform.
pub const PRESS_ON_DOWN: bool = true;

/// The tallest a drawn menu gets before its rows scroll: eleven rows and most of the twelfth.
pub(super) const MAX_H: f32 = 2.0 * MENU.inset + 11.5 * MENU.row_h;

/// A drawn menu's narrowest, as Desktop's: a dropdown's, and a right-click menu's.
pub(super) const MIN_W: f32 = 96.0;

/// The location card's rows (`session-identity-header.css`): 2 apart, 8 in on both sides.
pub(super) const CARD: popover::Look = popover::Look { row_gap: 2.0, pad_l: 8.0, pad_r: 8.0, ..MENU };

/// A select's list (`select.css`): 4 in around its rows and 2 between them, at most 12rem tall
/// (its rows scroll past that) and at least 160 wide.
pub(super) const SELECT: popover::Look = popover::Look { inset: 4.0, row_gap: 2.0, ..MENU };
pub(super) const SELECT_MAX_H: f32 = 192.0;
pub(super) const SELECT_MIN_W: f32 = 160.0;

pub(super) const CONTEXT_MIN_W: f32 = 160.0;

/// From a point (a right-click) or an element to the menu's corner.
pub(super) const GAP: f32 = 2.0;

/// A group's title (`menu-v2-group-label`): a 28 pt row, 11/530, faint, on the labels' edge.
pub(super) const GROUP_H: f32 = 28.0;
pub(super) const GROUP: TextStyle = TextStyle::new(11.0, 530.0).lh(14.0).tracking(0.05);

/// The root surface's key, which an open menu focuses.
pub(super) const SURFACE: &str = "popup-menu";

/// Host capabilities a Rune application supplies to the shared popup/menu engine.
///
/// Only [`popup`](Self::popup), [`popup_mut`](Self::popup_mut), [`knobs`](Self::knobs), and
/// [`clock`](Self::clock) are required. Optional hooks carry sensible defaults so feature crates
/// and minimal embedders need not supply desktop-only chrome.
pub trait PopupHost: Sized {
    /// Read-only access to the host's popup state (must not dirty `rune::Tracked` cells).
    fn popup(&self) -> &Popup<Self>;

    /// Mutable access to the host's popup state (called only when popup state actually changes).
    fn popup_mut(&mut self) -> &mut Popup<Self>;

    /// Popover motion knobs used for opening/closing transitions and placement.
    fn knobs(&self) -> &Knobs;

    /// Current frame clock in seconds, used to expire native AppKit repost-suppression windows.
    fn clock(&self) -> f64;

    /// Called when a drawn popup takes keyboard focus (either its search field or its menu
    /// surface), allowing the host to blur active text editors such as the session composer.
    fn on_focus_taken(&mut self) {}

    /// Maximum drawn menu width in window points (for narrow/mobile viewports). `None` leaves the
    /// menu's requested width unclamped.
    fn max_width(&self) -> Option<f32> {
        None
    }

    /// Renders a 16 pt project avatar badge for `dir` when an item requests [`Item::project`].
    fn project_badge(&self, _p: &Pal, _dir: &str) -> Option<El> {
        None
    }

    /// Renders the 16 pt server health mark for [`Item::status`]. Defaults to Desktop's standard
    /// dot, lock, loader, or warning mark.
    fn status_indicator(&self, p: &Pal, indicator: Indicator) -> El {
        rows::status_mark(p, indicator)
    }

    /// One device pixel in shell points for crisp 1 px rules in [`Menu::standard`] menus.
    /// Defaults to deriving the hairline from [`crate::scroller::device_scale`].
    fn hairline(&self) -> f32 {
        let scale = crate::scroller::device_scale();
        scale.floor().max(1.0) / scale
    }

    /// Whether a Rune modal surface (such as a dialog or media lightbox) is currently mounted
    /// beneath the drawn popup layer.
    ///
    /// When `true`, [`view`] wraps the drawn popup's scrim, point anchor, and menu surfaces in a
    /// raised modal scope so Rune's active-modal input filter allows hover, clicks, and outside
    /// dismissal to reach the popup instead of being blocked by the modal underneath. When
    /// `false`, the popup stays non-modal so context menus keep `hover_passes_below` and
    /// `press_focuses_below` on background chrome and `MenuButton` drag-to-pick gestures stay
    /// intact.
    fn over_modal(&self) -> bool {
        false
    }
}

pub(super) type Pick<S> = Box<dyn FnOnce(&mut S, Option<&str>, &mut Tx)>;
pub(super) type SearchRows<S> = Rc<dyn Fn(&S, &str) -> Menu>;

/// A search field over a drawn menu's rows, which `rows` rebuilds from what's typed.
pub(super) struct Search<S> {
    pub(super) placeholder: String,
    pub(super) rows: SearchRows<S>,
    /// A field that edits a value instead of filtering ([`open_with_field`]).
    pub(super) field: Option<Field<S>>,
}

impl<S> Clone for Search<S> {
    fn clone(&self) -> Self {
        Self { placeholder: self.placeholder.clone(), rows: Rc::clone(&self.rows), field: self.field.clone() }
    }
}

/// What an editing field draws before its text, what each edit does, and what choosing a radio
/// row does (the menu stays open, as Kobalte's `Menu.RadioItem` leaves it).
pub(super) struct Field<S> {
    pub(super) lead: Rc<dyn Fn(&S, &Pal) -> El>,
    pub(super) changed: Rc<dyn Fn(&mut S, &str, &mut Tx)>,
    pub(super) radio: Rc<dyn Fn(&mut S, &str, &mut Tx)>,
}

impl<S> Clone for Field<S> {
    fn clone(&self) -> Self {
        Self { lead: Rc::clone(&self.lead), changed: Rc::clone(&self.changed), radio: Rc::clone(&self.radio) }
    }
}

/// The drawn menu showing now.
pub(super) struct Shown<S> {
    pub(super) menu: Menu,
    pub(super) anchor: PopupAnchor,
    /// The lit row, by pointer or keys.
    pub(super) active: Option<usize>,
    /// The submenus open beside their rows, outermost first: one opened from the root's row,
    /// then one from a row of that, and so on.
    pub(super) subs: Vec<SubOpen>,
    /// How it opened and from what: where focus goes as it closes.
    pub(super) opened: focus::Opened,
    /// Live search builder when opened with [`open_searchable`].
    pub(super) search: Option<Search<S>>,
    /// Optional callback run when a root-level item becomes highlighted by pointer or keys.
    pub(super) highlight: Option<Rc<dyn Fn(&mut S, &str)>>,
    pub(super) pick: Pick<S>,
}

impl<S> Shown<S> {
    /// Whether the open menu takes focus, as Kobalte's do: not one whose search field keeps it.
    /// A menu under an editing field takes it as any menu does; the field takes it when pressed.
    pub(super) fn holds_focus(&self) -> bool {
        self.search.as_ref().is_none_or(|x| x.field.is_some())
    }

    /// The menu at `level`: the root (0), or the submenu open that deep.
    pub(super) fn menu_at(&self, level: usize) -> Option<&Menu> {
        self.subs.get(..level)?.iter().try_fold(&self.menu, |m, o| m.sub_at(o.at))
    }

    /// The lit row of the menu at `level`.
    pub(super) fn active_at(&self, level: usize) -> Option<usize> {
        if level == 0 { self.active } else { self.subs.get(level - 1)?.active }
    }

    pub(super) fn set_active(&mut self, level: usize, n: usize) {
        if level == 0 {
            self.active = Some(n);
        } else if let Some(o) = self.subs.get_mut(level - 1) {
            o.active = Some(n);
        }
    }

    /// The menu the keys move in: the deepest open submenu with a lit row, else the root.
    pub(super) fn key_level(&self) -> usize {
        self.subs.iter().take_while(|o| o.active.is_some()).count()
    }
}

/// A submenu open beside row `at`, row `active` of it lit. The keys move in it once a row of
/// it is lit, as the pointer leaves them in the menu that opened it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct SubOpen {
    pub(super) at: usize,
    pub(super) active: Option<usize>,
}

/// The drawn menu, while one is open. Its callback runs once: with the picked id, or `None`
/// when it's dismissed.
pub struct Popup<S> {
    pub(super) shown: Option<Shown<S>>,
    /// A searchable menu's field.
    pub(super) query: TextEdit,
    /// The element focused as a [`Menu::remembers_focus`] menu opened.
    pub(super) remembered: Option<String>,
    /// A menu button's mouse press in progress ([`MenuButton`]).
    pub(super) press: trigger::Press,
    /// State of a native AppKit pop-up menu, so a second press on its trigger closes rather than
    /// reopening it.
    pub(super) native: native::State,
    /// The offered rows of this window's drawn menu, deepest level last, with their window rects
    /// as its view last built: a layout read only works while the view builds, and `pick_at`
    /// runs from an input handler.
    pub(super) rows: RefCell<Vec<(String, [f32; 4])>>,
    /// Each open submenu's edge facing the menu that opened it, by that menu's level, as the view
    /// last built: where a pointer leaving its row may be heading.
    pub(super) targets: RefCell<Vec<submenu::Target>>,
    /// The pointer's last two positions over the drawn menu, in window points (earlier, latest).
    /// Kept out of the view's state: a move alone redraws nothing.
    pub(super) pointer: std::cell::Cell<[[f32; 2]; 2]>,
    /// Whether an open submenu outlives the pointer crossing its menu ([submenu]).
    pub(super) submenu: submenu::State,
}

impl<S> Default for Popup<S> {
    fn default() -> Self {
        Self::new()
    }
}

impl<S> Popup<S> {
    pub fn new() -> Self {
        Self {
            shown: None,
            query: TextEdit::default(),
            remembered: None,
            press: trigger::Press::Idle,
            native: native::State::Closed,
            rows: RefCell::new(Vec::new()),
            targets: RefCell::new(Vec::new()),
            pointer: std::cell::Cell::new([[0.0; 2]; 2]),
            submenu: submenu::State::Steady,
        }
    }

    pub fn is_open(&self) -> bool {
        self.shown.is_some()
    }

    /// A menu showing now holds the keyboard (Kobalte's focus their items, outside every pane).
    pub fn holds_focus(&self) -> bool {
        self.shown.as_ref().is_some_and(Shown::holds_focus)
    }

    /// The menu showing now is one under an editing field ([`open_with_field`]).
    pub(super) fn edits(&self) -> bool {
        self.shown.as_ref().is_some_and(|m| m.search.as_ref().is_some_and(|x| x.field.is_some()))
    }

    /// The element focused as the last [`Menu::remembers_focus`] menu opened: its Edit items
    /// put focus back there before they send their keys.
    pub fn remembered(&self) -> Option<&str> {
        self.remembered.as_deref()
    }

    /// Whether the menu showing now is anchored at element `key`.
    pub fn open_at(&self, key: &str) -> bool {
        self.shown.as_ref().is_some_and(|m| matches!(&m.anchor, PopupAnchor::Key(k) if k == key))
    }
}
