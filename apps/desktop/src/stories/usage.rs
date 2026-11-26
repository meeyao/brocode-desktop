//! The storybook's Usage dialog (`Dialogs/Usage`) and `--scene usage`, rendered by the built-in
//! Luau `usage` plugin (`plugins/usage/`) with Desktop's story data
//! (`usage/dialog.standalone.stories.tsx`): an Acme workspace with OpenCode Go, an Anthropic
//! budget and a shared pool; a per-model budget the model in use reached; the model budget or
//! the workspace's reached; out of credits; no budgets; and signed out.
//! `--playground Dialogs/Usage --variant <name> --whole-window --snap out.png`.

use rune::{El, Play, col};
use serde_json::{Value, json};

use crate::App;
use crate::palette::Pal;
use crate::plugin_host::{OpenDialog, services};
use crate::stories::{Story, Trigger, replay, stage};

const VARIANTS: &[(&str, usize)] = &[
    ("Default", 0),
    ("Per model", 1),
    ("Model budget reached", 2),
    ("Workspace budget reached", 3),
    ("Out of credits", 4),
    ("No budgets", 5),
    ("Signed out", 6),
];

const KEYS: &[Trigger] = &[replay(open)];

const HOUR: i64 = 3_600_000;

fn open(s: &mut App, _: &mut Play<App>) {
    setup(s);
}

fn setup(s: &mut App) {
    s.finish_stream();
    let resets_at = session_view::turn_errors::parse_iso("2026-11-01T00:00:00Z").unwrap_or_default();
    let model = |id: &str, name: &str, spent: Option<f64>| {
        let mut m = json!({ "providerID": "opencode", "modelID": id, "name": name });
        if let Some(spent) = spent {
            m["spent"] = json!(spent);
        }
        m
    };
    let opus = |spent| model("claude-opus-5", "Claude Opus 5", spent);
    let sonnet = |spent| model("claude-sonnet-5", "Claude Sonnet 5", spent);
    let anthropic = |spent: f64| {
        json!({
            "id": "anthropic",
            "limit": 150.0,
            "spent": spent,
            "scope": "each",
            "coverage": "models",
            "vendors": ["Anthropic"],
            "models": [opus(None), sonnet(None)],
            "resetsAt": resets_at,
        })
    };
    let pool = |spent: f64| {
        json!({
            "id": "pool",
            "limit": 1000.0,
            "spent": spent,
            "coverage": "all",
            "shared": true,
            "resetsAt": resets_at,
        })
    };
    let per_model = json!({
        "id": "each",
        "limit": 20.0,
        "spent": 20.0,
        "scope": "each",
        "per": "model",
        "coverage": "models",
        "picked": true,
        "models": [opus(Some(20.0)), sonnet(Some(4.5))],
        "resetsAt": resets_at,
    });
    let now = *s.now_ms;
    let meter = |percent: f64, hours: i64| json!({ "percent": percent, "limited": false, "resetsAt": now + hours * HOUR });
    let info = |budgets: Vec<Value>, go: bool| {
        let mut obj = json!({
            "workspace": {
                "name": "Acme",
                "usageURL": "https://apibrother.com",
            },
            "budgets": budgets,
        });
        if go {
            obj["go"] = json!({
                "rolling": meter(42.0, 3),
                "weekly": meter(18.0, 100),
                "monthly": meter(7.0, 500),
            });
        }
        obj
    };
    let current = json!({
        "provider": "opencode",
        "model": "claude-opus-5",
        "name": "Claude Opus 5",
    });

    let variant = *s.fixture_run.variant;
    let data = match variant {
        1 => json!({ "key": variant, "load": "ready", "info": info(vec![per_model], false), "model": current, "credits": false }),
        2 => json!({ "key": variant, "load": "ready", "info": info(vec![anthropic(150.42), pool(394.57)], false), "model": current, "credits": false }),
        3 => json!({ "key": variant, "load": "ready", "info": info(vec![anthropic(80.0), pool(1000.12)], false), "credits": false }),
        4 => json!({ "key": variant, "load": "ready", "info": info(vec![anthropic(80.0), pool(394.57)], false), "credits": true }),
        5 => json!({ "key": variant, "load": "ready", "info": info(Vec::new(), false), "credits": false }),
        6 => json!({ "key": variant, "load": "signed-out", "credits": false }),
        _ => json!({ "key": variant, "load": "ready", "info": info(vec![anthropic(123.14), pool(394.57)], true), "credits": false }),
    };

    if s.fixture_run.stage.is_some() {
        crate::plugin_host::boot_builtin(s, "usage");
    }
    s.plugins.dialog = Some(OpenDialog {
        plugin: "usage".into(),
        id: "main".into(),
        data: services::json_to_data(&data),
    });
}

/// The dialog alone, centred on the stage.
fn view(s: &App, p: &Pal) -> El {
    let card = crate::plugin_host::dialog_card(s, p, [1280.0, 640.0]).map(|(_, el)| el);
    col().key("story-usage").w_full().h(640.0).center().fill(p.bg_base).children(card)
}

rune_register::register!(crate::stories::STORY_LIST: Story = [
    Story { path: "Dialogs/Usage", scene: |s| stage(s, view), knobs: &[], keys: KEYS, variants: VARIANTS },
]);

// `--scene usage`: the Default dialog over the app, to see it in a short window, where its
// content scrolls.
rune_register::register!(crate::scenes::SCENE_LIST: crate::scenes::Scene = [
    ("usage", setup),
]);
