//! Settings › Preferences (`settings/general/general.tsx`): Interface layout, Language, Default
//! environment, Auto-accept permissions, Show agent, Follow-up behavior, Submit question answers
//! with, Pinch to zoom and Spellcheck, this app's own rows (Terminal placement, macOS Option-as-Meta, Windows shell
//! prompt marks), and the sections plugins put on the page (`PluginSettingsSections page="general"`).

use opencode_i18n::t;
use opencode_ui::focus::FocusExt;
use opencode_ui::keymap::Bind;
use opencode_ui::popup::PopupHost;
use rune::{Color, El, Tx, col, row, text};
use opencode_ui::palette::Pal;
use opencode_ui::style::UI;

use super::{choice, header_with, page_with, select};
use crate::host::SettingsHost;
use crate::types::{DockPlacement, TerminalMarkShell};
use crate::{Choice as _, FollowUp, QuestionSubmitKey, StartIn, TitlebarPosition, Worktree, change_settings, list, set_show_agent, set_terminal_placement, setting, setting_at, switch};

fn toggle_shared_side_panel(s: &mut dyn SettingsHost) {
    change_settings(s, |x| x.shared_side_panel = !x.shared_side_panel);
}

fn pick_language(s: &mut dyn SettingsHost, id: &str, _tx: &mut Tx) {
    s.set_language(id);
}

fn pick_start_in(s: &mut dyn SettingsHost, id: &str, _tx: &mut Tx) {
    change_settings(s, |x| x.start_in = StartIn::parse(Some(id)));
}

fn pick_worktree(s: &mut dyn SettingsHost, id: &str, _tx: &mut Tx) {
    change_settings(s, |x| x.worktree = Worktree::parse(Some(id)));
}

/// New sessions start in, and under Current project the Worktree it uses. Default directory
/// names the folder the server reports for itself (`GET /api/location`) once it's known.
fn start_rows<H: SettingsHost + PopupHost + 'static>(s: &H, p: &Pal) -> Vec<(&'static str, El)> {
    let st = s.settings();
    let title = t!("rune.settings.newSession.startIn.title");
    let about = match (st.start_in, s.server_directory()) {
        (StartIn::Directory, Some(dir)) => t!("rune.settings.newSession.startIn.directoryDescription", directory = dir).to_string(),
        (StartIn::Directory, None) => t!("rune.settings.newSession.startIn.directoryUnknown").to_owned(),
        (StartIn::Project, _) => t!("rune.settings.newSession.startIn.description").to_owned(),
    };
    let mut rows = vec![("settings-new-session-start", setting(p, "settings-new-session-start", title, &about, choice(s, p, "settings-new-session-start-select", title, st.start_in, pick_start_in)))];
    if st.start_in == StartIn::Project {
        let title = t!("rune.settings.newSession.worktree.title");
        let about = match st.worktree {
            Worktree::Default => t!("rune.settings.newSession.worktree.defaultDescription"),
            Worktree::New => t!("rune.settings.newSession.worktree.newDescription"),
            Worktree::LastUsed => t!("rune.settings.newSession.worktree.lastUsedDescription"),
        };
        rows.push(("settings-new-session-worktree", setting(p, "settings-new-session-worktree", title, about, choice(s, p, "settings-new-session-worktree-select", title, st.worktree, pick_worktree))));
    }
    rows
}

fn toggle_auto_accept(s: &mut dyn SettingsHost) {
    let on = !s.settings().auto_accept;
    s.set_auto_accept(on);
}

fn toggle_show_agent(s: &mut dyn SettingsHost) {
    let on = !s.settings().show_agent;
    set_show_agent(s, on);
}

fn pick_follow_up(s: &mut dyn SettingsHost, id: &str, _tx: &mut Tx) {
    change_settings(s, |x| x.follow_up = FollowUp::parse(Some(id)));
}

fn pick_question_submit(s: &mut dyn SettingsHost, id: &str, _tx: &mut Tx) {
    change_settings(s, |x| x.question_submit = QuestionSubmitKey::parse(Some(id)));
}

/// A chord as Desktop's `formatKeybind` writes it in a sentence ("Enter", "Ctrl+Enter").
fn key_words(chord: &str) -> String {
    rune::Keybind::new(chord).chords().first().map_or_else(String::new, |c| opencode_ui::keymap::base::sentence(*c))
}

fn toggle_pinch_zoom(s: &mut dyn SettingsHost, tx: &mut Tx) {
    let on = !s.settings().pinch_zoom;
    s.set_pinch_zoom(on, tx);
}

fn toggle_spellcheck(s: &mut dyn SettingsHost) {
    change_settings(s, |x| x.spellcheck = !x.spellcheck);
}

fn toggle_mobile_diff_wrap(s: &mut dyn SettingsHost) {
    change_settings(s, |x| x.mobile_diff_wrap = !x.mobile_diff_wrap);
}

fn toggle_mobile_titlebar(s: &mut dyn SettingsHost) {
    change_settings(s, |x| {
        x.mobile_titlebar = if x.mobile_titlebar == TitlebarPosition::Bottom { TitlebarPosition::Top } else { TitlebarPosition::Bottom };
    });
}

fn toggle_terminal_option_meta(s: &mut dyn SettingsHost) {
    change_settings(s, |x| x.terminal_option_meta = !x.terminal_option_meta);
}

fn toggle_terminal_mark(s: &mut dyn SettingsHost, id: &'static str) {
    change_settings(s, |x| {
        if x.terminal_marks.iter().any(|m| m == id) {
            x.terminal_marks.retain(|m| m != id);
        } else {
            x.terminal_marks.push(id.to_owned());
        }
    });
}

fn interface_rows<H: SettingsHost + PopupHost + 'static>(s: &H, p: &Pal) -> Vec<(&'static str, El)> {
    let st = s.settings();
    if !st.navigation_prototype() {
        let control = crate::interface_mode::control(s, p);
        let row = setting_at(p, "settings-tab-layout", t!("settings.general.row.interfaceMode.title"), t!("settings.general.row.interfaceMode.description"), control, false);
        return vec![("settings-tab-layout", row)];
    }
    let control = crate::navigation_mode::control(s, p);
    let mut rows = vec![(
        "settings-navigation",
        setting_at(p, "settings-navigation", t!("settings.general.row.navigation.title"), t!("settings.general.row.navigation.description"), control, false),
    )];
    if st.navigation() == crate::Navigation::Tabs {
        let title = t!("settings.general.row.tabPosition.title");
        let control = select(s, p, "settings-tab-position", title, crate::navigation_mode::positions(), st.tab_position().name(), crate::navigation_mode::pick_position);
        rows.push(("settings-tab-position", setting(p, "settings-tab-position", title, "", control)));
        let title = t!("settings.general.row.tabGrouping.title");
        let control = select(s, p, "settings-tab-grouping", title, crate::navigation_mode::groupings(), st.tab_grouping().name(), crate::navigation_mode::pick_grouping);
        rows.push(("settings-tab-grouping", setting(p, "settings-tab-grouping", title, "", control)));
    }
    rows
}

pub fn view<H: SettingsHost + PopupHost + 'static>(s: &H, p: &Pal) -> El {
    let st = s.settings();
    let alternate = Bind::SubmitAlternate.chord().map_or_else(String::new, opencode_ui::keymap::base::sentence);
    let other_enter = key_words(st.question_submit.other());
    let mut rows = interface_rows(s, p);
    rows.extend([
        (
            "settings-shared-side-panel",
            setting(
                p,
                "settings-shared-side-panel",
                t!("settings.general.row.sharedSidePanel.title"),
                t!("settings.general.row.sharedSidePanel.description"),
                switch(p, "shared-side-panel-switch", st.shared_side_panel).on_press(|s: &mut H| toggle_shared_side_panel(s)),
            ),
        ),
        (
            "settings-language",
            setting(
                p,
                "settings-language",
                t!("settings.general.row.language.title"),
                t!("settings.general.row.language.description"),
                select(
                    s,
                    p,
                    "settings-language-select",
                    t!("settings.general.row.language.title"),
                    opencode_i18n::locales().iter().map(|l| (l.id().to_owned(), l.label().to_owned())).collect(),
                    opencode_i18n::locale().id(),
                    pick_language,
                ),
            ),
        ),
    ]);
    rows.extend(start_rows(s, p));
    rows.extend([
        (
            "settings-auto-accept-permissions",
            setting(
                p,
                "settings-auto-accept-permissions",
                t!("command.permissions.autoaccept.enable"),
                t!("settings.permissions.autoAccept.default.description"),
                switch(p, "auto-accept-switch", st.auto_accept).on_press(|s: &mut H| toggle_auto_accept(s)),
            ),
        ),
        (
            "settings-show-custom-agents",
            setting(
                p,
                "settings-show-custom-agents",
                t!("settings.general.row.showCustomAgents.title"),
                t!("settings.general.row.showCustomAgents.description"),
                switch(p, "show-agent", st.show_agent).on_press(|s: &mut H| toggle_show_agent(s)),
            ),
        ),
        (
            "settings-follow-up-behavior",
            setting(
                p,
                "settings-follow-up-behavior",
                t!("settings.general.row.followUpBehavior.title"),
                &t!("settings.general.row.followUpBehavior.description", keybind = alternate),
                choice(s, p, "settings-follow-up-behavior-select", t!("settings.general.row.followUpBehavior.title"), st.follow_up, pick_follow_up),
            ),
        ),
        // `QuestionSubmitKeySetting` (`settings/general/general.tsx:140-168`): each choice is
        // named by its key, and the description names the Enter that does the other thing.
        (
            "settings-question-submit-key",
            setting(
                p,
                "settings-question-submit-key",
                t!("settings.general.row.questionSubmitKey.title"),
                &t!("settings.general.row.questionSubmitKey.description", keybind = other_enter),
                select(
                    s,
                    p,
                    "settings-question-submit-key-select",
                    t!("settings.general.row.questionSubmitKey.title"),
                    QuestionSubmitKey::ALL.into_iter().map(|k| (k.name().to_owned(), key_words(k.name()))).collect(),
                    st.question_submit.name(),
                    pick_question_submit,
                ),
            ),
        ),
        (
            "settings-pinch-zoom",
            setting(
                p,
                "settings-pinch-zoom",
                t!("settings.general.row.pinchZoom.title"),
                t!("settings.general.row.pinchZoom.description"),
                switch(p, "pinch-zoom-switch", st.pinch_zoom).on_press_tx(|s: &mut H, tx: &mut Tx| toggle_pinch_zoom(s, tx)),
            ),
        ),
        (
            "settings-spellcheck",
            setting(
                p,
                "settings-spellcheck",
                t!("settings.general.row.spellcheck.title"),
                t!("settings.general.row.spellcheck.description"),
                switch(p, "spellcheck-switch", st.spellcheck).on_press(|s: &mut H| toggle_spellcheck(s)),
            ),
        ),
        (
            "settings-mobile-diff-wrap",
            setting(
                p,
                "settings-mobile-diff-wrap",
                t!("plugin.review.settings.wrapLines.title"),
                t!("plugin.review.settings.wrapLines.description"),
                switch(p, "mobile-diff-wrap-switch", st.mobile_diff_wrap).on_press(|s: &mut H| toggle_mobile_diff_wrap(s)),
            ),
        ),
    ]);
    if s.narrow() {
        let bottom = st.mobile_titlebar == TitlebarPosition::Bottom;
        let control = switch(p, "mobile-titlebar-bottom-switch", bottom).on_press(|s: &mut H| toggle_mobile_titlebar(s));
        rows.push((
            "settings-mobile-titlebar-bottom",
            setting(p, "settings-mobile-titlebar-bottom", t!("settings.general.row.mobileTitlebarBottom.title"), t!("settings.general.row.mobileTitlebarBottom.description"), control),
        ));
    }
    let placement_help = t!("rune.settings.general.row.terminalPlacement.description");
    let placement_help = if cfg!(windows) {
        format!("{placement_help} {}", t!("rune.settings.general.row.terminalPlacement.wslFolder"))
    } else {
        placement_help.to_owned()
    };
    rows.push(("terminal-placement", setting(p, "terminal-placement", t!("rune.settings.general.row.terminalPlacement.title"), &placement_help, placement(s, p))));
    if cfg!(target_os = "macos") {
        let control = switch(p, "terminal-option-meta-switch", st.terminal_option_meta).on_press(|s: &mut H| toggle_terminal_option_meta(s));
        rows.push(("terminal-option-meta", setting(p, "terminal-option-meta", t!("rune.settings.general.row.terminalOptionMeta.title"), t!("rune.settings.general.row.terminalOptionMeta.description"), control)));
    }
    if cfg!(windows) {
        for shell in TerminalMarkShell::ALL {
            let id = shell.id();
            let on = st.terminal_marks.iter().any(|m| m == id);
            let control = switch(p, &format!("{}-switch", shell.setting_key()), on).on_press(move |s: &mut H| toggle_terminal_mark(s, id));
            let description = match shell {
                TerminalMarkShell::Cmd => t!("rune.settings.general.row.terminalMarks.descriptionCmd").to_owned(),
                TerminalMarkShell::GitBash | TerminalMarkShell::Wsl => t!("rune.settings.general.row.terminalMarks.descriptionBash", shell = shell.label()),
                TerminalMarkShell::Pwsh | TerminalMarkShell::PowerShell => t!("rune.settings.general.row.terminalMarks.description", shell = shell.label()),
            };
            let title = t!("rune.settings.general.row.terminalMarks.title", shell = shell.label());
            rows.push((shell.setting_key(), setting(p, shell.setting_key(), &title, &description, control)));
        }
    }
    let rows = super::rows(s, p, 20.0, rows);
    let general = super::section(p, "settings-general-section", Some(t!("settings.general.section.general")), list(p, "settings-list", rows));
    let body = col()
        .w_full()
        .gap(32.0)
        .child(general)
        .child(super::timeline::section(s, p))
        .children(s.plugin_sections_for_page(p, "general"));
    page_with(
        p,
        "settings-general-page",
        header_with(p, t!("settings.tab.preferences"), t!("settings.preferences.description"), Some(help_feedback_link::<H>(p))),
        body,
        true,
    )
}

/// Official Preferences header link (`general.tsx:335-343`, `external-link.tsx:13`):
/// `ExternalLink` adds `underline` which wins over `no-underline` in Tailwind v4 cascade, so the
/// link rests underlined in `text-muted` and shifts to `text-base` on hover/focus.
fn help_feedback_link<H: SettingsHost + 'static>(p: &Pal) -> El {
    const URL: &str = "https://github.com/meeyao/brocode-desktop/issues/new";
    let words = t!("settings.helpFeedback");
    row()
        .key("settings-help-feedback")
        .role("link")
        .label(words)
        .gap(6.0)
        .center_y()
        .radius(4.0)
        .focus_default(p)
        .hover_group_focus()
        .on_press_tx(|s: &mut H, tx: &mut Tx| crate::open_external(s, URL, tx))
        .child(
            col()
                .key("help-label")
                .child(text(words).font(super::NAME_REGULAR).color(p.muted).group_color(p.text))
                .child(
                    rune::el()
                        .key("help-underline")
                        .pin(Some(0.0), None, Some(0.0), Some(1.0))
                        .h(1.0)
                        .fill(p.muted)
                        .child(rune::el().pin(Some(0.0), Some(0.0), Some(0.0), Some(0.0)).fill(p.text).group_opacity(0.0, 1.0)),
                ),
        )
        .child(rune::icon(&super::console_link::ARROW_UP_RIGHT, 14.0).color(p.muted).group_color(p.text))
}

/// Side or Bottom: where the terminal opens.
fn placement<H: SettingsHost + 'static>(s: &H, p: &Pal) -> El {
    let mut r = row()
        .key("terminal-placement-control")
        .role("radiogroup")
        .labelled_by("title")
        .focus_group()
        .gap(2.0)
        .padding(2.0)
        .radius(6.0)
        .fill(p.layer2);
    for (pl, label) in [
        (DockPlacement::Side, t!("rune.settings.general.row.terminalPlacement.side")),
        (DockPlacement::Bottom, t!("rune.settings.general.row.terminalPlacement.bottom")),
    ] {
        let on = s.settings().terminal_placement == pl;
        r = r.child(
            row()
                .key(format!("placement-{}", pl.name()))
                .role("radio")
                .focus_selected(on)
                .focus_default(p)
                .h(24.0)
                .px(10.0)
                .center()
                .radius(4.0)
                .fill(if on { p.bg_base } else { Color::TRANSPARENT })
                .hover(if on { p.bg_base } else { p.hover })
                .on_press_tx(move |s: &mut H, tx: &mut Tx| set_terminal_placement(s, pl, tx))
                .child(text(label).font(UI).color(if on { p.text } else { p.muted })),
        );
    }
    r
}
