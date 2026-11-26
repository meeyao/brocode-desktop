//! App-level shortcuts that a focused web page yields to Brocode rather than swallowing,
//! matching official `packages/gui-plugins/src/browser/keys.ts:34-97` (`pageShortcut`).

use crate::keymap::Bind;
use rune::{Key, KeyEvent};

pub fn claims_app_shortcut(ev: &KeyEvent) -> bool {
    if PAGE_KEYS.iter().any(|c| c.matches(ev)) {
        return false;
    }
    if crate::shortcuts::claims_from_browser(ev) {
        return true;
    }
    if [Bind::Rename, Bind::Background].iter().any(|b| b.matches(ev)) || crate::keymap::TERMINAL_TOGGLE.matches(ev) {
        return true;
    }
    let mod_only = if cfg!(target_os = "macos") { ev.mods.cmd && !ev.mods.ctrl } else { ev.mods.ctrl && !ev.mods.cmd };
    match ev.key {
        Key::Function(3) if !cfg!(target_os = "macos") && !ev.mods.ctrl && !ev.mods.cmd && !ev.mods.alt => true,
        Key::Function(5) if !cfg!(target_os = "macos") && !ev.mods.cmd && !ev.mods.alt && !ev.mods.shift => true,
        Key::Tab if ev.mods.ctrl && !ev.mods.cmd && !ev.mods.alt => true,
        Key::Left | Key::Right if mod_only && ev.mods.alt && !ev.mods.shift => true,
        Key::Command(c) => {
            let ch = c.to_ascii_lowercase();
            if ev.mods.alt {
                return !ev.mods.shift && matches!(ch, 'r' | 'b' | 'p');
            }
            if ev.mods.shift {
                if ch == 'm' && !matches!(crate::updater::build_channel(), "stable" | "prod" | "latest" | "beta") {
                    return true;
                }
                return matches!(ch, 't' | '[' | ']' | '{' | '}' | 'r' | 'g' | 'p' | 'l' | 'e' | 'o' | 's' | 'c');
            }
            matches!(ch, 'l' | 't' | 'n' | 'w' | 'f' | 'g' | '1'..='9' | ',' | '\\')
        }
        _ => false,
    }
}

static PAGE_KEYS: std::sync::LazyLock<Vec<rune::Chord>> = std::sync::LazyLock::new(|| {
    ["mod+a", "mod+c", "mod+v", "mod+x", "mod+z", "mod+r", "mod+shift+z"]
        .into_iter()
        .filter_map(|c| rune::Chord::parse(c).ok())
        .collect()
});
