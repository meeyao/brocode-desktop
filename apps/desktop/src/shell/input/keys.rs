use rune::{Key, KeyEvent, Tx};

use crate::pickers::PickerHost as _;
use crate::{App, Route, knobs};

pub(crate) fn on_key(s: &mut App, ev: &KeyEvent, tx: &mut Tx) -> bool {
    s.sidebar_inbox.focus.note_key(ev);
    // A key chord in progress, or one this key starts, takes the key before anything else
    // (`createAppKeymap` reads keys on the document, in the capture phase).
    if crate::key_sequence::window_key(s, ev, tx) {
        return true;
    }
    crate::tab_hints::key_down(s, ev, tx);
    // A key before the after-frame turn still finds the dormant built-ins' shortcuts.
    crate::plugin_host::doze_owed(s);
    crate::status_toast::hotkey(s, ev, tx);
    // A key from inside the side region (something there has the keyboard) wakes its tab's label.
    if s.kernel.slots.get(&crate::panels::SIDE).focus.is_quiet() && tx.focused().is_some() && !s.composer.focused {
        s.kernel.slots.get_mut(&crate::panels::SIDE).focus.wake_on_key();
    }
    if overlays_and_shortcuts(s, ev, tx, true) || global(s, ev, tx) {
        return true;
    }
    if ev.key == Key::Escape {
        return crate::escape::key(s, ev, tx);
    }
    match &ev.key {
        // Unmodified Page Up and Page Down page the timeline wherever the pointer and focus are,
        // as a chat pages its messages from the composer; modified page keys and active IME
        // composition keep their native meaning (`session/composer/region.tsx:174-184`).
        Key::PageUp | Key::PageDown
            if !(ev.mods.shift || ev.mods.cmd || ev.mods.alt || ev.mods.ctrl) && !s.composer.is_composing() =>
        {
            match &*s.route {
                Route::Session { id, .. } => {
                    tx.scroll_page(format!("scroll-{id}"), if ev.key == Key::PageDown { 1 } else { -1 });
                    true
                }
                _ => false,
            }
        }
        _ => crate::type_to_compose::key(s, ev, tx),
    }
}

/// Runs on the timeline's scroll viewport ahead of Rune's `caret_browse_key` when the chat
/// timeline holds focus (`tx.focused() == Some("timeline")`). Intercepts only keys that
/// `caret_browse_key` would otherwise consume so active overlays and window shortcuts (such as
/// `Alt+Up/Down`, `Alt+Home`, `Mod+Alt+Left/Right`) win while caret/selection keys
/// (`Shift+Arrow*`, `Arrow*`, `Home`/`End`, `PageUp`/`PageDown`, `Cmd/Ctrl+A/C`) reach Rune.
pub(crate) fn timeline_key(s: &mut App, ev: &KeyEvent, tx: &mut Tx) -> bool {
    if tx.focused() != Some("timeline") || !is_caret_key(ev) {
        return false;
    }
    if crate::key_sequence::window_key(s, ev, tx) {
        return true;
    }
    crate::tab_hints::key_down(s, ev, tx);
    crate::plugin_host::doze_owed(s);
    overlays_and_shortcuts(s, ev, tx, false) || global(s, ev, tx)
}

fn is_caret_key(ev: &KeyEvent) -> bool {
    matches!(
        &ev.key,
        Key::Left
            | Key::Right
            | Key::Up
            | Key::Down
            | Key::Home
            | Key::End
            | Key::PageUp
            | Key::PageDown
            | Key::Command('a' | 'c')
    ) || matches!(ev.control_binding(), Some('a' | 'b' | 'e' | 'f' | 'p' | 'n'))
}

fn overlays_and_shortcuts(s: &mut App, ev: &KeyEvent, tx: &mut Tx, include_panels: bool) -> bool {
    // Every declared key band in `overlays::key_band::ALL` must be dispatched below.
    const _: () = assert!(crate::overlays::key_band::ALL.len() == 4);
    let overlays = s.kernel.overlay_dispatcher.clone();
    overlays.handle_key_band(crate::overlays::key_band::POPUP, s, ev, tx)
        || overlays.handle_key_band(crate::overlays::key_band::MODAL, s, ev, tx)
        || crate::glitch::key(s, ev, tx)
        || crate::demo::key(s, ev, tx)
        || overlays.handle_key_band(crate::overlays::key_band::MID, s, ev, tx)
        || crate::dictation::key(s, ev, tx)
        || knobs::key(s, ev, tx)
        || overlays.handle_key_band(crate::overlays::key_band::LOW, s, ev, tx)
        || (include_panels && s.panels_key(ev, tx))
        || (include_panels && s.opening.is_none() && s.question_key(ev, tx))
        || s.add_key(ev, tx)
        || s.draft_location_key(ev, tx)
        || crate::shortcuts::run_window_accelerators(s, ev, tx)
        || s.shortcut_without_dictation(ev, tx)
        || s.picker_key(ev, tx)
}

/// The window's commands that a focused field doesn't keep for itself, as Desktop matches its
/// command keybinds on the document before the field (`command.tsx` `handleKeyDown`, capture):
/// the composer and Home search ask for them first too, so Alt+Home goes Home from them.
pub(crate) fn global(s: &mut App, ev: &KeyEvent, tx: &mut Tx) -> bool {
    crate::shortcuts::run_global(s, ev, tx)
}
