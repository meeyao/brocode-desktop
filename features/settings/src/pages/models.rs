//! Settings › Models (`settings/models/models.tsx`): every model the server lists, by provider,
//! each with a switch that shows or hides it in the model list, a search over them, and each
//! provider folding shut.

use std::cell::OnceCell;
use std::collections::HashSet;

use opencode_i18n::t;
use opencode_ui::focus::FocusExt;
use opencode_ui::popup::{self, Item, Menu, MenuButton as _, PopupHost};
use opencode_ui::widgets::{self, badge};
use rune::{Align, Color, El, Tx, col, el, icon, row, rule, text};
use rune_kit::Switch;
use rune_theme_opencode::icons as ic;
use serde_json::{Value, json};
use opencode_ui::palette::{Pal, kit};
use opencode_kit::model::Model;

use super::{NAME, NAME_REGULAR, catalog, filter_field, header_with, page, tab_no_results};
use crate::host::{ProviderRow, SettingsHost};
use crate::stored;

const CONSOLE_GROUP_KEY: &str = "console:opencode";
/// How long a Providers -> Models reveal waits for its provider to appear (`MODELS_REVEAL_TIMEOUT_MS`).
const REVEAL_TIMEOUT_MS: i64 = 5_000;

/// Pending provider reveal started from Settings › Providers (`settings/models/machine.ts`).
#[derive(Default)]
pub enum Reveal {
    #[default]
    Idle,
    Waiting { provider: String, deadline_ms: i64, expanded: bool },
    Scroll { provider: String },
}

/// Console workspace providers detected from the connected catalog (`providers/catalog/console.ts`).
pub(super) struct ConsoleGroup {
    pub workspace: String,
    pub prefix: String,
    pub root_id: String,
    pub ids: HashSet<String>,
    pub providers: Vec<ProviderRow>,
}

pub(super) fn console_group(s: &dyn SettingsHost) -> Option<ConsoleGroup> {
    let providers = s.provider_rows(None);
    let root = providers.iter().find(|p| p.id == "opencode" && p.integration.as_deref() == Some("opencode"))?;
    let workspace = root.name.strip_suffix(" / OpenCode")?.trim();
    if workspace.is_empty() {
        return None;
    }
    let prefix = format!("{workspace} / ");
    let grouped: Vec<ProviderRow> = providers
        .iter()
        .filter(|p| p.integration.as_deref() == Some("opencode") && p.name.starts_with(&prefix))
        .cloned()
        .collect();
    if grouped.is_empty() {
        return None;
    }
    let ids = grouped.iter().map(|p| p.id.clone()).collect();
    Some(ConsoleGroup {
        workspace: workspace.to_owned(),
        prefix,
        root_id: root.id.clone(),
        ids,
        providers: grouped,
    })
}

/// Switches to Settings › Models, clears its filter, expands the path to `provider`, and scrolls
/// and focuses its section header (`models.tsx:124-198, 269-285`).
pub(super) fn open_for_provider(s: &mut dyn SettingsHost, provider: &str, tx: &mut Tx) {
    s.settings_pages_mut().search = rune::TextEdit::new("");
    super::open_page(s, super::Page::Models, tx);
    let deadline_ms = s.now_ms() + REVEAL_TIMEOUT_MS;
    s.settings_pages_mut().models_reveal = Reveal::Waiting { provider: provider.to_owned(), deadline_ms, expanded: false };
    poll(s, tx);
}

pub(crate) fn cancel_reveal(s: &mut dyn SettingsHost) {
    s.settings_pages_mut().models_reveal = Reveal::Idle;
}

pub fn poll(s: &mut dyn SettingsHost, tx: &mut Tx) -> bool {
    if !s.settings_open() || s.settings_pages().page != super::Page::Models {
        if !matches!(s.settings_pages().models_reveal, Reveal::Idle) {
            s.settings_pages_mut().models_reveal = Reveal::Idle;
        }
        return false;
    }
    let state = std::mem::take(&mut s.settings_pages_mut().models_reveal);
    match state {
        Reveal::Idle => false,
        Reveal::Waiting { provider, deadline_ms, expanded } => {
            // Typing a search filter cancels the reveal so it never fights the user (`machine.ts:98`).
            if !s.settings_pages().search.text.trim().is_empty() || s.now_ms() >= deadline_ms {
                return false;
            }
            if !s.models().iter().any(|m| m.provider == provider) {
                s.settings_pages_mut().models_reveal = Reveal::Waiting { provider, deadline_ms, expanded };
                return false;
            }
            if !expanded {
                let in_console = console_group(s).is_some_and(|cg| cg.ids.contains(&provider));
                if in_console {
                    Folds::expand(s, &[CONSOLE_GROUP_KEY, provider.as_str()]);
                } else {
                    Folds::expand(s, &[provider.as_str()]);
                }
            }
            s.settings_pages_mut().models_reveal = Reveal::Scroll { provider };
            tx.instant();
            true
        }
        Reveal::Scroll { provider } => {
            tx.instant();
            tx.scroll_into_view(format!("settings-models-provider-{provider}"));
            tx.focus(format!("settings-models-trigger-{provider}"), true);
            true
        }
    }
}

/// The providers folded shut, kept per server under Desktop's global
/// `settings-v2.models.providers` (`collapsed`, by provider): read the first time Models
/// draws, written on each fold.
#[derive(Default)]
pub struct Folds(OnceCell<HashSet<String>>);

impl Folds {
    fn get(&self, s: &dyn SettingsHost) -> &HashSet<String> {
        self.0.get_or_init(|| {
            server(s)
                .map(|server| {
                    read()[server]["collapsed"]
                        .as_object()
                        .into_iter()
                        .flatten()
                        .filter(|(_, shut)| shut.as_bool() == Some(true))
                        .map(|(provider, _)| provider.clone())
                        .collect()
                })
                .unwrap_or_default()
        })
    }

    /// Folds `provider` shut, or open again, and saves the folds.
    fn toggle(s: &mut dyn SettingsHost, provider: &str) {
        let mut shut = s.settings_pages().collapsed.get(s).clone();
        if !shut.remove(provider) {
            shut.insert(provider.to_owned());
        }
        Self::save(s, shut);
    }

    /// Expands only the specified keys so the saved layout of other providers is kept (`models.tsx:135`).
    fn expand(s: &mut dyn SettingsHost, providers: &[&str]) {
        let mut shut = s.settings_pages().collapsed.get(s).clone();
        let mut changed = false;
        for p in providers {
            changed |= shut.remove(*p);
        }
        if changed {
            Self::save(s, shut);
        }
    }

    fn save(s: &mut dyn SettingsHost, shut: HashSet<String>) {
        if let Some(server) = server(s) {
            let collapsed: serde_json::Map<String, Value> = shut.iter().map(|p| (p.clone(), Value::Bool(true))).collect();
            opencode_platform::storage::update_json(stored::GLOBAL, KEY, |all| {
                all.insert(server, json!({ "collapsed": collapsed }));
            });
        }
        s.settings_pages_mut().collapsed = Folds(OnceCell::from(shut));
    }
}

const KEY: &str = "settings-v2.models.providers";

fn read() -> Value {
    opencode_platform::storage::read_json(stored::GLOBAL, KEY).unwrap_or_else(|| json!({}))
}

/// The server the folds are kept for; none while nothing persists (headless, a fixture).
fn server(s: &dyn SettingsHost) -> Option<String> {
    s.persists().then(|| s.live_server_base()).flatten()
}

// `packages/app/src/providers/catalog/order.ts` (`opencode#66736`).
const POPULAR: [&str; 9] = ["opencode-go", "opencode", "openai", "anthropic", "claude-agent", "github-copilot", "google", "openrouter", "vercel"];

/// The models the search finds, each provider's together by name, the providers in Desktop's
/// order: the popular ones first, then by name.
fn groups(s: &dyn SettingsHost) -> Vec<(String, Vec<&Model>)> {
    let query = s.settings_pages().search.text.trim().to_owned();
    let mut models: Vec<&Model> = s
        .models()
        .iter()
        // A model's speeds are not entries of their own (`settings/models/models.tsx` `listed`).
        .filter(|m| m.speed.is_none())
        .filter(|m| query.is_empty() || opencode_search::model_search::score(&query, &[m.group.as_str(), m.name.as_str(), m.id.as_str()]).is_some())
        .collect();
    let rank = |m: &Model| match POPULAR.iter().position(|p| *p == m.provider) {
        Some(at) => (at, String::new()),
        None => (POPULAR.len(), m.group.to_lowercase()),
    };
    models.sort_by(|a, b| rank(a).cmp(&rank(b)).then_with(|| rune::text::compare_names(&a.name, &b.name)));
    let mut out: Vec<(String, Vec<&Model>)> = Vec::new();
    for m in models {
        match out.last_mut() {
            Some((p, list)) if *p == m.provider => list.push(m),
            _ => out.push((m.provider.clone(), vec![m])),
        }
    }
    out
}

fn toggle_provider_fold(s: &mut dyn SettingsHost, provider: &str, tx: &mut Tx) {
    tx.instant();
    Folds::toggle(s, provider);
}

fn toggle_model_visibility(s: &mut dyn SettingsHost, key: &str, visible: bool, tx: &mut Tx) {
    tx.instant();
    s.set_model_visibility(key, visible);
}

fn set_provider_visibility(s: &mut dyn SettingsHost, provider: &str, visible: bool, tx: &mut Tx) {
    tx.instant();
    let keys: Vec<String> = s.models().iter().filter(|m| m.provider == provider && m.speed.is_none()).map(|m| m.key.clone()).collect();
    for key in keys {
        s.set_model_visibility(&key, visible);
    }
}

pub fn view<H: SettingsHost + PopupHost + 'static>(s: &H, p: &Pal) -> El {
    let searching = !s.settings_pages().search.text.trim().is_empty();
    let groups = groups(s);
    let managed = console_group(s);
    let mut body = col().key("settings-models").w_full().gap(12.0);
    if groups.is_empty() {
        let status = if searching {
            tab_no_results(p, "settings-models-empty", &s.settings_pages().search.text)
        } else {
            row().key("settings-models-empty").w_full().py(48.0).center().child(text(t!("dialog.model.empty")).font(NAME_REGULAR).color(p.muted))
        };
        body = body.child(status);
    } else {
        let mut emitted_console = false;
        for (provider, models) in &groups {
            if let Some(cg) = &managed
                && cg.ids.contains(provider)
            {
                if !emitted_console {
                    emitted_console = true;
                    let nested: Vec<(&String, &Vec<&Model>)> = groups.iter().filter(|(id, _)| cg.ids.contains(id)).map(|(id, ms)| (id, ms)).collect();
                    let open = searching || !s.settings_pages().collapsed.get(s).contains(CONSOLE_GROUP_KEY);
                    body = body.child(console_section(s, p, cg, &nested, open, searching));
                }
                continue;
            }
            let open = searching || !s.settings_pages().collapsed.get(s).contains(provider);
            body = body.child(group(s, p, provider, models, open, searching));
        }
    }
    let head = col()
        .w_full()
        .gap(24.0)
        .child(header_with(p, t!("settings.models.title"), t!("settings.models.description"), super::servers::switcher(s, p)))
        .child(filter_field::<H>(p, "settings-models-search", t!("dialog.model.search.placeholder"), &s.settings_pages().search, |st| &mut st.search));
    page(p, "settings-models-page", head, body)
}

fn section_header_trigger<H: SettingsHost + 'static>(
    p: &Pal,
    key: &str,
    fold_key: String,
    icon_el: El,
    title: String,
    badge_text: Option<&str>,
    open: bool,
    searching: bool,
) -> El {
    let mut label = row().gap(8.0).center_y().child(icon_el).child(text(title.clone()).font(NAME).color(p.text));
    if let Some(ws) = badge_text {
        label = label.child(badge(p, ws));
    }
    let trigger = row()
        .key(key.to_owned())
        .role("button")
        .label(title)
        .expanded(open)
        .h(28.0)
        .pt(4.0)
        .pr(8.0)
        .pb(4.0)
        .pl(4.0)
        .gap(4.0)
        .center_y()
        .radius(4.0)
        .focus_default(p)
        .child(
            el().size(20.0, 20.0)
                .center()
                // `.settings-models-group-chevron` (`provider-group.tsx:100`): default 16 pt `<Icon name="chevron-down" />`.
                .child(icon(&ic::CHEVRON_DOWN, 16.0).color(p.icon_muted).rotate(if open { 0.0 } else { -std::f32::consts::FRAC_PI_2 }).mirror_in_rtl()),
        )
        .child(label);
    if searching {
        trigger.disabled(true)
    } else {
        trigger.hover(p.layer2).on_press_tx(move |s: &mut H, tx: &mut Tx| toggle_provider_fold(s, &fold_key, tx))
    }
}

/// OpenCode Console workspace section (`ProviderModelSections` managed branch, `provider-group.tsx:172-218`):
/// collapsible `OpenCode` + workspace badge over indented provider cards with a clickable collapse rail.
fn console_section<H: SettingsHost + PopupHost + 'static>(
    s: &H,
    p: &Pal,
    cg: &ConsoleGroup,
    nested: &[(&String, &Vec<&Model>)],
    open: bool,
    searching: bool,
) -> El {
    let trigger = section_header_trigger::<H>(
        p,
        "trigger",
        CONSOLE_GROUP_KEY.to_owned(),
        widgets::provider_mark("opencode", p.icon),
        t!("provider.connect.opencode.name").to_owned(),
        Some(&cg.workspace),
        open,
        searching,
    );
    let mut section = col()
        .key("settings-models-console")
        .w_full()
        .gap(12.0)
        .pb(if open { 12.0 } else { 0.0 })
        .child(row().key("settings-models-group-header").w_full().h(28.0).center_y().child(trigger));
    if open {
        let mut cards = col().w_full().gap(8.0);
        let count = nested.len();
        for (idx, (provider, models)) in nested.iter().enumerate() {
            let expanded = searching || !s.settings_pages().collapsed.get(s).contains(*provider);
            let card = console_provider_card(s, p, cg, provider, models, expanded, searching);
            let card = if expanded && idx + 1 < count { card.mb(4.0) } else { card };
            cards = cards.child(card);
        }
        let mut rail = el()
            .key("settings-models-console-rail")
            .pin(Some(-4.0), Some(0.0), None, Some(0.0))
            .w(20.0)
            .hover_group()
            .child(el().pin(Some(4.0), Some(0.0), None, Some(0.0)).w(1.0).fill(p.border).group_opacity(0.5, 1.0));
        if !searching {
            rail = rail.on_press_tx(|s: &mut H, tx: &mut Tx| toggle_provider_fold(s, CONSOLE_GROUP_KEY, tx));
        }
        section = section.child(col().key("settings-models-console-groups").w_full().ml(14.0).pl(16.0).child(rail).child(cards));
    }
    section
}

/// Nested card for a Console-managed provider (`ProviderModelGroup`, `provider-group.tsx:225-293`),
/// with its `⋯` menu offering Enable all / Disable all models.
fn console_provider_card<H: SettingsHost + PopupHost + 'static>(
    s: &H,
    p: &Pal,
    cg: &ConsoleGroup,
    provider: &str,
    models: &[&Model],
    open: bool,
    searching: bool,
) -> El {
    let raw_name = models.first().map_or_else(|| provider.to_owned(), |m| m.group.clone());
    let name = raw_name.strip_prefix(&cg.prefix).unwrap_or(&raw_name).to_owned();
    let fold = provider.to_owned();
    let anchor = format!("settings-models-group-menu-{provider}");
    let menu_open = s.popup().open_at(&anchor);
    let trigger = row()
        .key(format!("settings-models-trigger-{provider}"))
        .role("button")
        .label(name.clone())
        .expanded(open)
        .grow(1.0)
        .shrink()
        .min_w(0.0)
        .min_h(48.0)
        .px(16.0)
        .py(12.0)
        .gap(8.0)
        .center_y()
        .radius(8.0)
        .focus_inset(p)
        .child(widgets::provider_mark(provider, p.icon))
        .child(text(name).font(NAME).color(p.text).truncate().shrink().min_w(0.0))
        .child(
            icon(&ic::CHEVRON_DOWN, 14.0)
                .color(p.icon_muted)
                .rotate(if open { 0.0 } else { -std::f32::consts::FRAC_PI_2 })
                .mirror_in_rtl(),
        );
    let trigger = if searching {
        trigger.disabled(true)
    } else {
        trigger.on_press_tx(move |s: &mut H, tx: &mut Tx| toggle_provider_fold(s, &fold, tx))
    };
    let provider_for_menu = provider.to_owned();
    let anchor_click = anchor.clone();
    let dots = el()
        .key(anchor.clone())
        .role("button")
        .label_static(t!("common.moreOptions"))
        .expanded(menu_open)
        .size(24.0, 24.0)
        .radius(4.0)
        .center()
        .focus_default(p)
        .fill(if menu_open { p.pressed } else { Color::TRANSPARENT })
        .hover(if menu_open { p.pressed } else { p.hover })
        .press(p.pressed)
        .menu_button(anchor, move |s: &mut H, tx: &mut Tx| {
            let menu = Menu::new(t!("common.moreOptions"))
                .align(Align::End)
                .gap(4.0)
                .item(Item::new("enable", t!("settings.models.enableAll")))
                .item(Item::new("disable", t!("settings.models.disableAll")));
            let pid = provider_for_menu.clone();
            popup::open(s, menu, anchor_click.clone(), tx, move |s: &mut H, picked, tx| match picked {
                Some("enable") => set_provider_visibility(s, &pid, true, tx),
                Some("disable") => set_provider_visibility(s, &pid, false, tx),
                _ => {}
            });
        })
        .child(icon(&ic::OUTLINE_DOTS, 16.0).color(p.icon_muted));
    let header = row()
        .key("header")
        .w_full()
        .center_y()
        .radius(8.0)
        .fill(if menu_open { p.layer2 } else { Color::TRANSPARENT })
        .hover(p.layer2)
        .child(trigger)
        .child(row().pr(12.0).center_y().child(dots));
    let mut card = col()
        .key(format!("settings-models-provider-{provider}"))
        .w_full()
        .radius(8.0)
        .fill(p.layer1)
        .box_shadow(Pal::ring_in(p.border_muted))
        .clip_rounded(8.0)
        .child(header);
    if open {
        let n = models.len();
        let mut inner = col().w_full().px(16.0).child(rule(0.5, p.border));
        for (at, m) in models.iter().enumerate() {
            inner = inner.child(model_row(s, p, m));
            if at + 1 < n {
                inner = inner.child(rule(0.5, p.border));
            }
        }
        card = card.child(inner);
    }
    card
}

/// A direct provider's section: its header folds it; its models in a catalog list with their switches.
fn group<H: SettingsHost + 'static>(s: &H, p: &Pal, provider: &str, models: &[&Model], open: bool, searching: bool) -> El {
    // Only the keyless catalog is free; a Zen key or Console account keeps the provider's own name (`provider-group.tsx:96-99`).
    let name = if provider == "opencode" && models.iter().all(|m| m.free) {
        t!("provider.connect.opencode.freeName").to_owned()
    } else {
        models.first().map_or_else(|| provider.to_owned(), |m| m.group.clone())
    };
    let trigger = section_header_trigger::<H>(p, &format!("settings-models-trigger-{provider}"), provider.to_owned(), widgets::provider_mark(provider, p.icon), name, None, open, searching);
    let mut section = col()
        .key(format!("settings-models-provider-{provider}"))
        .w_full()
        .gap(12.0)
        .pb(if open { 12.0 } else { 0.0 })
        .child(row().key("settings-models-group-header").w_full().h(28.0).center_y().child(trigger));
    if open {
        let rows = models.iter().map(|m| model_row(s, p, m)).collect();
        section = section.child(catalog(p, "settings-models-list", rows));
    }
    section
}

fn model_row<H: SettingsHost + 'static>(s: &H, p: &Pal, m: &Model) -> El {
    let shown = s.model_visible(m);
    let key = m.key.clone();
    row()
        .key(format!("settings-model-{}", m.key))
        .w_full()
        .py(16.0)
        .gap(16.0)
        .center_y()
        .child(text(m.name.clone()).font(NAME_REGULAR).color(p.text).grow(1.0).shrink().min_w(0.0).truncate().help_when_truncated(m.name.clone()).help_above())
        .child(
            Switch::new(shown)
                .web()
                .label(m.name.clone())
                .hide_label()
                .on_toggle_tx(move |s: &mut H, tx: &mut Tx| toggle_model_visibility(s, &key, !shown, tx))
                .build(kit(p)),
        )
}
