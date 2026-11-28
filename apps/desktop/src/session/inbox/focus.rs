//! Focus rescue state machine for the sidebar inbox (`packages/ui/src/context/focus-rescue.tsx`,
//! `packages/app/src/shell/titlebar/inbox.tsx`, `opencode#65690`, `opencode#66547`).
//!
//! Tracks whether the last focus-moving gesture was a keyboard navigation key
//! (`Tab` / `Arrow*` / `Home` / `End` without Ctrl/Alt/Cmd) or pointer/non-navigation input, and
//! tracks the focused inbox row's section, container, epoch, and visible sibling order. When that
//! row remounts (section move, Shelf <-> Inline toggle, virtual-list epoch change) or disappears
//! (delete, archive, section collapse), `step` returns a focus effect whose ring and scroll
//! behaviour match `focus-rescue.tsx` (`focusVisible: ring, preventScroll: !ring`).

use rune::machine::Effects;
use rune::{Key, KeyEvent};

use crate::App;
use crate::inbox_order::{Folds, Group, Order, inbox_successor};
use crate::restore::{InboxUi, WorkingPlacement};

/// Whether keyboard navigation keys drove focus last (`focus-rescue.tsx:64-73`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Modality {
    #[default]
    Pointer,
    Keyboard,
}

/// Which inbox card holds focus and where it stood when focused.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Track {
    #[default]
    Idle,
    Row {
        key: String,
        group: Group,
        in_shelf: bool,
        epoch: String,
        order: Vec<String>,
    },
}

/// Complete state of sidebar inbox focus rescue.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FocusRescue {
    modality: Modality,
    track: Track,
}

pub enum FocusEvent<'a> {
    /// Any key event observed in capture phase (`focus-rescue.tsx:66-69`).
    Key(&'a KeyEvent),
    /// Any pointer press (`focus-rescue.tsx:71-73`).
    Pointer,
    /// An inbox card gained focus.
    Focused {
        key: String,
        group: Group,
        in_shelf: bool,
        epoch: String,
        order: Vec<String>,
        ring: bool,
    },
    /// The inbox's visible row order changed while a card may still be focused.
    OrderUpdated { order: Vec<String> },
    /// An inbox card lost focus; decide whether to follow a remount or rescue a removed/collapsed row.
    Blurred {
        key: &'a str,
        now_group: Option<Group>,
        now_in_shelf: bool,
        now_epoch: &'a str,
        now_order: &'a [String],
        suppressed: bool,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FocusEffect {
    /// Move focus to `key`; draw its ring when `ring` and scroll it into view only when `scroll`.
    Focus { key: String, ring: bool, scroll: bool },
}

/// `NAVIGATION` in `packages/ui/src/context/focus-rescue.tsx:64` (`opencode#65690`):
/// Tab, arrows, Home, and End without Ctrl/Alt/Meta. Typing, Enter, PageUp/PageDown, and shortcuts
/// never turn the focus ring on for a rescued control.
pub fn is_navigation_key(ev: &KeyEvent) -> bool {
    !(ev.mods.ctrl || ev.mods.alt || ev.mods.cmd || ev.mods.logo)
        && matches!(ev.key, Key::Tab | Key::Up | Key::Down | Key::Left | Key::Right | Key::Home | Key::End)
}

/// Pure state transition for [`FocusRescue`].
pub fn step(state: &mut FocusRescue, event: FocusEvent<'_>) -> Effects<FocusEffect> {
    match event {
        FocusEvent::Key(ev) => {
            if is_navigation_key(ev) {
                state.modality = Modality::Keyboard;
            }
            Effects::none()
        }
        FocusEvent::Pointer => {
            state.modality = Modality::Pointer;
            Effects::none()
        }
        FocusEvent::Focused { key, group, in_shelf, epoch, order, ring } => {
            if !ring {
                state.modality = Modality::Pointer;
            }
            state.track = Track::Row { key, group, in_shelf, epoch, order };
            Effects::none()
        }
        FocusEvent::OrderUpdated { order } => {
            if let Track::Row { key, order: saved, .. } = &mut state.track
                && order.iter().any(|k| k == key)
            {
                *saved = order;
            }
            Effects::none()
        }
        FocusEvent::Blurred { key, now_group, now_in_shelf, now_epoch, now_order, suppressed } => {
            let prev = match &state.track {
                Track::Row { key: k, .. } if k == key => std::mem::take(&mut state.track),
                _ => return Effects::none(),
            };
            let Track::Row { key, group, in_shelf, epoch, order } = prev else {
                return Effects::none();
            };
            if suppressed {
                return Effects::none();
            }
            // The card stayed mounted in the same container/section and epoch: focus moved away
            // deliberately (pointer click or key navigation elsewhere), so do not rescue it.
            if now_group == Some(group) && now_in_shelf == in_shelf && (in_shelf || now_epoch == epoch) {
                return Effects::none();
            }
            let ring = state.modality == Modality::Keyboard;
            // Remounted elsewhere (section change, Shelf <-> Inline, or virtual-list epoch):
            // follow the same key (`focus-rescue.tsx:171` `keyed(spot.key)`, `opencode#66547`).
            if now_group.is_some() {
                return Effects::one(FocusEffect::Focus { key, ring, scroll: ring });
            }
            // Removed or collapsed: rescue to the nearest surviving visible row below, else above,
            // without scrolling when `!ring` (`focus-rescue.tsx:155,194`).
            let next = inbox_successor(&order, &key, |k| now_order.iter().any(|v| v == k));
            match next {
                Some(target) => Effects::one(FocusEffect::Focus { key: target, ring, scroll: ring }),
                None => Effects::none(),
            }
        }
    }
}

impl FocusRescue {
    /// Whether rescued or follow-up focus should show its keyboard focus ring.
    pub fn ring(&self) -> bool {
        self.modality == Modality::Keyboard
    }

    pub fn note_key(&mut self, ev: &KeyEvent) {
        let _ = step(self, FocusEvent::Key(ev));
    }

    pub fn note_pointer(&mut self) {
        let _ = step(self, FocusEvent::Pointer);
    }

    pub fn note_order(&mut self, order: Vec<String>) {
        let _ = step(self, FocusEvent::OrderUpdated { order });
    }
}

/// Which group and container (`in_shelf`) currently show card `key` (`ib-...`), if visible.
pub(crate) fn card_location(order: &Order, folds: Folds, placement: WorkingPlacement, key: &str) -> (Option<Group>, bool) {
    if order.shows(Group::Working, folds).iter().any(|&at| order.rows[at].key == key) {
        return (Some(Group::Working), placement == WorkingPlacement::Shelf);
    }
    for g in [Group::Current, Group::Needs, Group::Recent, Group::Archived] {
        if order.shows(g, folds).iter().any(|&at| order.rows[at].key == key) {
            return (Some(g), false);
        }
    }
    (None, false)
}

/// Visible inbox card keys (`ib-...`) in display order for `order`, `ui`, and `drafts`.
pub(crate) fn visible_card_keys_from(order: &Order, ui: InboxUi, drafts: &[String]) -> Vec<String> {
    let folds = Folds::from(ui);
    let mut out = Vec::with_capacity(drafts.len() + order.rows.len());
    out.extend(drafts.iter().map(|id| format!("ib-{id}")));
    let push_group = |out: &mut Vec<String>, g: Group| {
        out.extend(order.shows(g, folds).into_iter().map(|at| order.rows[at].key.clone()));
    };
    push_group(&mut out, Group::Current);
    push_group(&mut out, Group::Needs);
    push_group(&mut out, Group::Recent);
    match ui.working_placement {
        WorkingPlacement::Inline => {
            push_group(&mut out, Group::Working);
            push_group(&mut out, Group::Archived);
        }
        WorkingPlacement::Shelf => {
            push_group(&mut out, Group::Archived);
            push_group(&mut out, Group::Working);
        }
    }
    out
}

/// Visible inbox card keys (`ib-...`) in display order from `app`.
pub(crate) fn visible_card_keys(app: &App) -> Vec<String> {
    let order = app.inbox_order();
    let ui = app.kernel.slots.get(&crate::restore::RESTORE).inbox;
    let drafts = crate::view::inbox::draft_ids(app);
    visible_card_keys_from(&order, ui, &drafts)
}
