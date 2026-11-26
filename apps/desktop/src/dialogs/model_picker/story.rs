//! The storybook's `Pickers/Model picker`: Desktop's model dialog with several providers, one
//! model under three of them, and three recent models under Recent. Its group headers say the
//! providers' names ("Anomaly / RITB"), as the live list's do. Its Efforts variant opens the lit
//! model's flyout beside it; its Favorite variant stars one model, so Favorites leads the list.
//! Its Starred variants add Nimbus 3 under three providers with the free one starred: under
//! Favorites with no search, by provider with a search, and with ⌘F starring a second as it
//! searches. R replays; O opens or closes the flyout; F presses ⌘F on the lit model.

use crate::pickers::{EffortFlyout as _, PickerHost as _};
use rune::Play;

use crate::App;
use crate::pickers::Kind;
use crate::stories::{Story, Trigger, replay};

const CURRENT: &str = "opencode:claude-opus-5-5";

const KEYS: &[Trigger] = &[
    replay(open),
    Trigger { key: 'o', label: "Open or close the efforts", run: |s, _| {
        if let Some(key) = s.picker_active() { s.toggle_effort_flyout(&key); }
    } },
    Trigger { key: 'f', label: "Press ⌘F on the lit model", run: |s, _| favorite_key(s) },
];

const VARIANTS: &[(&str, usize)] = &[
    ("List", 0), ("Efforts", 1), ("Favorite", 2), ("Keyboard budget", 3), ("Manage", 4),
    ("Starred", 5), ("Starred, searched", 6), ("Starred, ⌘F searching", 7),
];

/// The Starred variants' favorite: the free Nimbus 3, so its star sits beside Free.
const STARRED: &str = "opencode:nimbus-3";

/// ⌘F on the lit row, through the dialog's own keys.
fn favorite_key(s: &mut App) {
    let ev = rune::KeyEvent { key: rune::Key::Command('f'), mods: rune::Mods { cmd: true, ..Default::default() } };
    s.picker_key(&ev, &mut rune::Tx::default());
}

fn open(s: &mut App, _: &mut Play<App>) {
    s.finish_stream();
    let efforts = serde_json::json!([{"id": "low"}, {"id": "medium"}, {"id": "high"}, {"id": "xhigh"}]);
    let model = |id: &str, provider: &str, name: &str| serde_json::json!({"id": id, "providerID": provider, "name": name, "variants": efforts, "cost": [{"input": 3.0}]});
    let free = |id: &str, provider: &str, name: &str| serde_json::json!({"id": id, "providerID": provider, "name": name, "variants": efforts, "cost": [{"input": 0.0}]});
    let variant = *s.fixture_run.variant;
    let starred = variant >= 5;
    let mut models = serde_json::json!([
        model("claude-opus-5-5", "opencode", "Claude Opus 5.5"),
        model("claude-sonnet-5-5", "opencode", "Claude Sonnet 5.5"),
        model("gpt-6.1-terra", "github-copilot", "GPT-6.1 Terra"),
        model("gpt-6.1-terra", "openai", "GPT-6.1 Terra"),
        model("gpt-6-astra", "openai", "GPT-6 Astra"),
        model("gpt-6.1-terra", "console-openai-test", "GPT-6.1 Terra"),
        model("kimi-k3.1-pro", "console-moonshot-ai", "Kimi K3.1 Pro"),
        model("gemini-4-carbon", "console-google", "Gemini 4 Carbon"),
        model("grok-5", "xai", "Grok 5"),
    ]);
    if starred && let Some(list) = models.as_array_mut() {
        list.extend([
            free("nimbus-3", "opencode", "Nimbus 3"),
            model("nimbus-3", "acme", "Nimbus 3"),
            model("nimbus-3", "console-acme", "Nimbus 3"),
            model("nimbus-3-mini", "acme", "Nimbus 3 Mini"),
        ]);
    }
    s.composer_ext.pickers.load_live(&models, &serde_json::json!([{"id": "build"}]));
    // What the group headers say: `/api/provider`'s names, not the ids `/api/model` gives.
    s.composer_ext.pickers.load_providers(&serde_json::json!([
        { "id": "opencode", "name": "API Brother / Brocode" },
        { "id": "github-copilot", "name": "GitHub Copilot" },
        { "id": "openai", "name": "OpenAI" },
        { "id": "console-openai-test", "name": "Anomaly / RITB" },
        { "id": "console-moonshot-ai", "name": "Anomaly / Moonshot AI" },
        { "id": "console-google", "name": "Anomaly / Google" },
        { "id": "xai", "name": "xAI" },
        { "id": "acme", "name": "Acme" },
        { "id": "console-acme", "name": "Anomaly / Acme" },
    ]));
    s.composer_ext.pickers.prefs = Default::default();
    s.open_picker(Kind::Model);
    s.composer_ext.pickers.stage_effort(CURRENT, "xhigh");
    s.choose_picker(CURRENT);
    s.composer_ext.pickers.prefs.recent = ["console-openai-test:gpt-6.1-terra", "openai:gpt-6-astra", "xai:grok-5"].map(str::to_owned).to_vec();
    if s.fixture_run.variant == 2 {
        s.composer_ext.pickers.prefs.favorites = vec!["openai:gpt-6-astra".to_owned()];
    }
    if starred {
        s.composer_ext.pickers.prefs.favorites = vec![STARRED.to_owned()];
    }
    s.composer.focused = false;
    if s.fixture_run.variant == 3 {
        let label = opencode_i18n::t!("usage.budget.left", amount = opencode_i18n::usd(72.0, 2));
        let mut badges = std::collections::HashMap::new();
        for id in ["opencode:claude-opus-5-5", "opencode:claude-sonnet-5-5"] {
            badges.insert(id.to_owned(), (label.clone(), false));
        }
        s.plugins.set_model_badges("story", 0, badges);
    } else {
        s.plugins.clear_model_badges("story", None);
    }
    s.open_picker(Kind::Model);
    if s.fixture_run.variant == 1 {
        s.toggle_effort_flyout(CURRENT);
    }
    if s.fixture_run.variant == 3 {
        s.composer_ext.pickers.activate(CURRENT.into());
        s.picker_key(&rune::KeyEvent { key: rune::Key::Right, mods: Default::default() }, &mut rune::Tx::default());
    }
    if s.fixture_run.variant == 4 {
        s.close_picker(false);
        s.composer_ext.pickers.open_manage();
    }
    if variant >= 6 {
        s.composer_ext.pickers.search = rune::TextEdit::new("nimbus");
        s.composer_ext.pickers.search.focused = true;
        s.composer_ext.pickers.query_changed();
    }
    if s.fixture_run.variant == 7 {
        s.composer_ext.pickers.activate("acme:nimbus-3".into());
        favorite_key(s);
    }
}

rune_register::register!(crate::stories::STORY_LIST: Story = [
    Story::composer("Pickers/Model picker", KEYS).variants(VARIANTS),
]);
