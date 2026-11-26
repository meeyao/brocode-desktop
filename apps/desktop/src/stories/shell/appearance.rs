//! The storybook's Settings › Appearance story: the Appearance rows alone, in each variant's
//! theme. T paints the next theme, M opens the real theme menu, U and C switch to the next UI
//! and code font (the whole window follows), R replays.

use rune::{El, Play, Tx, col};

use crate::App;
use crate::palette::Pal;
use crate::stories::{Trigger, replay};

pub const VARIANTS: &[(&str, usize)] = &[("Brocode", 0), ("Tokyonight", 1), ("Catppuccin", 2), ("Dracula", 3), ("Nord", 4)];
const THEMES: [&str; 5] = [crate::themes::DEFAULT, "tokyonight", "catppuccin", "dracula", "nord"];

pub const KEYS: &[Trigger] = &[
    replay(open),
    Trigger { key: 't', label: "Next theme", run: |s, tx| next(s, tx) },
    Trigger { key: 'm', label: "Open the theme menu", run: |s, tx| s.open_theme_menu(tx) },
    Trigger { key: 'u', label: "Next UI font", run: |s, tx| s.set_ui_font((s.settings.ui_font + 1) % crate::appearance::UI_FONTS.len(), tx) },
    Trigger { key: 'c', label: "Next code font", run: |s, tx| s.set_code_font((s.settings.code_font + 1) % crate::appearance::CODE_FONTS.len(), tx) },
];

fn open(s: &mut App, _: &mut Play<App>) {
    s.settings.theme = THEMES[s.fixture_run.variant.min(THEMES.len() - 1)].into();
}

fn next(s: &mut App, tx: &mut Tx) {
    let all = crate::themes::all();
    let id = all[(crate::themes::index(&s.settings.theme) + 1) % all.len()].id.clone();
    s.set_theme(&id, tx);
}

fn view(s: &App, p: &Pal) -> El {
    col().key("appearance-story").w(640.0).fill(p.bg_base).padding(24.0).child(crate::settings::pages::appearance::view(s, p))
}

rune_register::register!(crate::stories::STORY_LIST: crate::stories::Story = [
    crate::stories::Story { path: "Settings/Appearance", scene: |s| crate::stories::stage(s, view), knobs: &[], keys: KEYS, variants: VARIANTS },
]);
