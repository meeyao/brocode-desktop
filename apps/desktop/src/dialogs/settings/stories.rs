//! The storybook's Settings > Page story: the whole screen with its Back to app button. B presses
//! the button (it closes the page in the real app), R replays. And a scene per page
//! (`--scene settings-<page>`), plus settings search and a shortcut being recorded.

use rune::{El, col};

use super::pages::Page;
use crate::App;
use crate::layout_state::TabLayout;
use crate::palette::Pal;
use crate::stories::{Trigger, replay};

pub const KEYS: &[Trigger] = &[
    replay(|_, _| {}),
    Trigger { key: 'b', label: "Press Back to app", run: |s, tx| { tx.instant(); s.close_settings() } },
];

fn view(s: &App, p: &Pal) -> El {
    col().key("settings-story").w(1056.0).h(640.0).fill(p.bg_base).child(crate::settings::page(s, p, (1056.0, 640.0)))
}

/// Settings › Interface layout over the whole window, at rest in top tabs: S and T switch to
/// side tabs and back, and each tab flies between the titlebar and the sidebar under the page.
pub const LAYOUT_KEYS: &[Trigger] = &[
    replay(|s, _| {
        crate::open_scene(s);
        s.finish_stream();
        s.settings.tab_layout = TabLayout::Horizontal;
        *s.layout = crate::layout_state::Shell::at_rest(TabLayout::Horizontal);
        s.open_settings();
    }),
    Trigger { key: 's', label: "Side tabs", run: |s, tx| s.set_tab_layout(TabLayout::Vertical, tx) },
    Trigger { key: 't', label: "Top tabs", run: |s, tx| s.set_tab_layout(TabLayout::Horizontal, tx) },
];

rune_register::register!(crate::stories::STORY_LIST: crate::stories::Story = [
    crate::stories::Story { path: "Settings/Page", scene: |s| crate::stories::stage(s, view), knobs: &[], keys: KEYS, variants: &[] },
    crate::stories::Story { path: "Settings/Layout switch", scene: |_| {}, knobs: &[], keys: LAYOUT_KEYS, variants: &[] },
]);

fn providers_with_accounts(s: &mut App) {
    use crate::connect::catalog::ProviderInfo;
    use opencode_kit::integration::{Connection, Integration};
    let account = |id: &str, label: &str| Connection { credential: true, id: id.into(), label: label.into(), method: "oauth".into(), status: None };
    let integration = |id: &str, name: &str, connections| Integration { id: id.into(), name: name.into(), connections, ..Default::default() };
    let provider = |id: &str, name: &str, via: Option<&str>| ProviderInfo { id: id.into(), name: name.into(), integration: via.map(str::to_owned) };
    s.seed_catalogs(
        vec![
            integration("opencode", "Brocode", vec![account("cred-console", "Anomaly")]),
            integration("anthropic", "Anthropic", vec![account("cred-claude-work", "Work"), account("cred-claude-home", "Personal")]),
        ],
        vec![
            provider("opencode", "Brocode Zen", None),
            provider("opencode-go", "Brocode Go", Some("opencode")),
            provider("console-anomaly", "Anomaly", Some("opencode")),
            provider("anthropic", "Anthropic", None),
        ],
    );
}

/// The open session behind Settings, on `page`.
fn on(s: &mut App, page: Page) {
    crate::open_scene(s);
    s.finish_stream();
    s.open_settings_at(page);
}

// Settings with the sidebar hidden, and each page.
rune_register::register!(crate::scenes::SCENE_LIST: crate::scenes::Scene = [
    ("settings-sidebar-closed", |s| {
        crate::open_scene(s);
        s.finish_stream();
        crate::scenes::side_tabs(s);
        *s.sidebar_open = false;
        s.open_settings();
    }),
    ("settings-providers", |s| on(s, Page::Providers)),
    // Providers with the Console signed in (Zen, Go and a workspace share its account) and
    // Anthropic on two accounts: each row has its own account menu.
    ("settings-providers-accounts", |s| {
        on(s, Page::Providers);
        providers_with_accounts(s);
    }),
    ("settings-models", |s| on(s, Page::Models)),
    ("settings-extensions", |s| on(s, Page::Extensions)),
    // Extensions > Plugins matching official `app-dialog-settings--plugins` story (#64511).
    ("settings-extensions-plugins", |s| {
        use opencode_kit::model::{PluginFeatures, PluginInfo, PluginSource, PluginState};
        on(s, Page::Extensions);
        let sp = s.kernel.slots.get_mut(&super::SETTINGS_PAGES);
        sp.tab = super::pages::ExtensionsTab::Plugins;
        let pkg = |id: &str, target: &str, outdated: bool| PluginInfo {
            id: Some(id.into()),
            source: PluginSource::Package { target: target.into(), version: None, outdated, updating: false },
            features: PluginFeatures { server: true, ..Default::default() },
            state: PluginState::Active,
        };
        sp.extensions.plugins = Some(Ok(vec![
            pkg("wakatime", "opencode-wakatime@latest", true),
            pkg("notifier", "opencode-notifier@latest", true),
            pkg("review", "opencode-review@latest", false),
        ]));
    }),
    ("settings-projects", |s| on(s, Page::Projects)),
    ("settings-worktrees", |s| on(s, Page::Worktrees)),
    ("settings-preferences", |s| on(s, Page::General)),
    // Preferences with new sessions in a new worktree of the current project.
    ("settings-new-session-worktree", |s| {
        on(s, Page::General);
        s.settings.worktree = crate::settings::Worktree::New;
    }),
    // Preferences with new sessions in the server's default directory, as it reported it.
    ("settings-new-session-directory", |s| {
        on(s, Page::General);
        s.settings.start_in = crate::settings::StartIn::Directory;
        crate::start_location::answered(s, "/home/me".into());
    }),
    ("settings-appearance", |s| on(s, Page::Appearance)),
    ("settings-notifications", |s| on(s, Page::Notifications)),
    ("settings-shortcuts", |s| on(s, Page::Shortcuts)),
    ("settings-pairing", |s| on(s, Page::Plugin("pairing.pairing".into()))),
    ("settings-server", |s| on(s, Page::Servers)),
    ("settings-experimental", |s| on(s, Page::Experimental)),
    ("settings-about", |s| on(s, Page::About)),
    // Settings search for "notif": its results in the rail, Preferences beside them.
    ("settings-search", |s| {
        on(s, Page::General);
        let sp = s.kernel.slots.get_mut(&super::SETTINGS_PAGES);
        sp.find.query = rune::TextEdit::new("notif");
        sp.find.query.focused = true;
    }),
    // A result opened: Appearance with its Theme row lit, the results still in the rail.
    ("settings-search-opened", |s| {
        on(s, Page::General);
        s.kernel.slots.get_mut(&super::SETTINGS_PAGES).find.query = rune::TextEdit::new("theme");
        let hit = crate::settings::search::rank("theme", crate::settings::search::index(s)).into_iter().find(|e| e.target.as_deref() == Some("settings-theme"));
        if let Some(e) = hit {
            crate::settings::search::open(s, &e, &mut rune::Tx::default());
        }
    }),
    // Shortcuts recording Auto-accept permissions' chord.
    ("settings-shortcut-recording", |s| {
        on(s, Page::Shortcuts);
        s.kernel.slots.get_mut(&super::SETTINGS_PAGES).shortcuts.capture = crate::settings::pages::shortcuts::Capture::Recording { id: "permissions.autoaccept".into(), prefix: Vec::new() };
    }),
    // Providers disconnect confirmation dialog.
    ("provider-disconnect", |s| {
        s.open_settings_at(Page::Providers);
        super::pages::providers::step_accounts(
            s,
            super::pages::providers::Event::Ask(super::pages::providers::Removal::Provider {
                id: "anthropic".into(),
                name: "Anthropic".into(),
                description: opencode_i18n::t!("settings.providers.disconnect.description").into(),
            }),
            &mut rune::Tx::default(),
        );
    }),
]);
