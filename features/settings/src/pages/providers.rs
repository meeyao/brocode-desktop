//! Settings › Providers (`settings/providers/providers.tsx`): the providers the server runs, each
//! with how it connects (API key, account, environment, config) and its accounts or Disconnect,
//! then the popular providers to connect and Show more providers, which opens the connect list.

use opencode_i18n::t;
use opencode_kit::integration::Integration;
use opencode_ui::focus::FocusExt;
use opencode_ui::popup::{self, Item, Menu as PopupMenu, MenuButton as _, PopupHost};
use opencode_ui::widgets::{self, DialogSize, Fit, badge, button, dialog_window, sized_modal};
use rune::machine::Effects;
use rune::{Align, Color, Cursor, Cx, El, Key, KeyEvent, Link, LinkPaint, TextEdit, TextStyle, Tx, col, el, icon, row, spacer, text};
use rune_kit::ButtonVariant;
use rune_theme_opencode::icons as ic;
use opencode_ui::palette::{Pal, kit};

use super::{BODY, NAME, catalog, header_with, page, section};
use crate::PROVIDER_REMOVAL_OVERLAY_ID;
use crate::host::{ProviderRow, SettingsHost};
use crate::jobs::{SettingsJob, SettingsRequest};

/// `dialog-title`: 15 on 20 at 530, -0.13 tracking.
const DIALOG_TITLE: TextStyle = TextStyle::new(15.0, 530.0).lh(20.0).tracking(-0.13);

/// Desktop's popular providers, in its order (`catalog/order.ts`).
// `packages/app/src/providers/catalog/order.ts` (`opencode#66736`).
const POPULAR: [&str; 9] = ["opencode-go", "opencode", "openai", "anthropic", "claude-agent", "github-copilot", "google", "openrouter", "vercel"];

/// The Connected rows' account rename/removal state. At most one row's rename field or removal
/// confirmation is open.
#[derive(Default)]
pub enum Accounts {
    #[default]
    Idle,
    Renaming { provider: String, credential: String, field: Box<TextEdit> },
    Confirm(Removal),
}

#[derive(Clone, Debug)]
pub enum Removal {
    Provider { id: String, name: String, description: String },
    Account { credential: String, provider: String, label: String, last: bool },
}

pub enum Event {
    /// Page changed or menu closed.
    Close,
    /// Rename in the row's options menu: its active account becomes a field.
    Rename { provider: String, credential: String, label: String },
    Cancel,
    Submit,
    Ask(Removal),
}

pub enum Effect {
    Rename { credential: String, label: String },
    Remove(Removal),
}

impl Accounts {
    pub fn step(&mut self, event: Event) -> Effects<Effect> {
        match (std::mem::take(self), event) {
            (_, Event::Ask(target)) => {
                *self = Accounts::Confirm(target);
                Effects::none()
            }
            (Accounts::Confirm(target), Event::Submit) => Effects::one(Effect::Remove(target)),
            (Accounts::Confirm(_), Event::Cancel | Event::Close) => Effects::none(),
            (_, Event::Rename { provider, credential, label }) => {
                let mut field = TextEdit::new(&label);
                field.focused = true;
                *self = Accounts::Renaming { provider, credential, field: Box::new(field) };
                Effects::none()
            }
            (Accounts::Renaming { .. }, Event::Cancel | Event::Close) => Effects::none(),
            (Accounts::Renaming { credential, field, .. }, Event::Submit) => {
                let label = field.text.trim().to_owned();
                if label.is_empty() { Effects::none() } else { Effects::one(Effect::Rename { credential, label }) }
            }
            (state, _) => {
                *self = state;
                Effects::none()
            }
        }
    }

    pub fn blur(&mut self) {
        if let Accounts::Renaming { field, .. } = self {
            field.focused = false;
        }
    }

    pub(crate) fn field_mut(&mut self) -> Option<&mut TextEdit> {
        match self {
            Accounts::Renaming { field, .. } => Some(field.as_mut()),
            _ => None,
        }
    }
}

fn note(id: &str) -> Option<&'static str> {
    Some(match id {
        "opencode-go" => t!("dialog.provider.opencodeGo.tagline"),
        "anthropic" => t!("dialog.provider.anthropic.note"),
        "claude-agent" => t!("dialog.provider.claudeAgent.note"),
        "openai" => t!("dialog.provider.openai.note"),
        "google" => t!("dialog.provider.google.note"),
        "openrouter" => t!("dialog.provider.openrouter.note"),
        "vercel" => t!("dialog.provider.vercel.note"),
        _ if id.starts_with("github-copilot") => t!("dialog.provider.copilot.note"),
        _ => return None,
    })
}

/// Steps the Connected rows' account state machine and executes resulting server effects.
pub fn step_accounts(s: &mut dyn SettingsHost, event: Event, tx: &mut Tx) {
    tx.instant();
    let was_confirm = matches!(s.settings_pages().accounts, Accounts::Confirm(_));
    let effects = s.settings_pages_mut().accounts.step(event);
    let is_confirm = matches!(s.settings_pages().accounts, Accounts::Confirm(_));
    if !was_confirm && is_confirm {
        let return_focus = tx.focused().map(str::to_owned);
        s.on_overlay_opened(PROVIDER_REMOVAL_OVERLAY_ID, return_focus);
    } else if was_confirm && !is_confirm {
        s.on_overlay_closed(PROVIDER_REMOVAL_OVERLAY_ID);
    }
    for effect in effects {
        match effect {
            Effect::Remove(Removal::Provider { id, name, .. }) => disconnect_provider(s, &id, &name),
            Effect::Remove(Removal::Account { credential, provider, .. }) => remove_account(s, &credential, &provider, tx),
            Effect::Rename { credential, label } => rename_account(s, credential, label),
        }
    }
}

fn ask_disconnect_provider(s: &mut dyn SettingsHost, id: &str, name: &str, tx: &mut Tx) {
    let providers = s.provider_rows(None);
    let count = providers.iter().filter(|pi| pi.integration.as_deref().unwrap_or(&pi.id) == id).count();
    let description = if id == "opencode" && count > 1 {
        let workspace = s
            .integrations(None)
            .unwrap_or_default()
            .iter()
            .find(|i| i.id == id)
            .and_then(|i| i.connections.iter().find(|c| c.credential))
            .map_or(t!("usage.workspace.fallback"), |c| c.label.as_str());
        opencode_i18n::plural!("settings.providers.disconnect.consoleDescription", count, workspace = workspace)
    } else {
        t!("settings.providers.disconnect.description").to_owned()
    };
    step_accounts(s, Event::Ask(Removal::Provider { id: id.to_owned(), name: name.to_owned(), description }), tx);
}

fn open_connect_from_settings(s: &mut dyn SettingsHost, provider: Option<&str>, tx: &mut Tx) {
    s.open_connect(provider, true, tx);
}

fn add_account(s: &mut dyn SettingsHost, id: &str, tx: &mut Tx) {
    step_accounts(s, Event::Close, tx);
    s.open_connect(Some(id), true, tx);
}

fn rename_key(s: &mut dyn SettingsHost, ev: &KeyEvent, tx: &mut Tx) -> bool {
    if ev.key != Key::Escape || ev.mods.cmd || ev.mods.ctrl || ev.mods.alt || ev.mods.shift {
        return false;
    }
    step_accounts(s, Event::Cancel, tx);
    true
}

/// `DialogConfirmProviderRemoval` (#64212), shared by Disconnect and Remove account.
pub fn confirmation<H: SettingsHost + 'static>(s: &H, p: &Pal, cx: &Cx<H>) -> Vec<El> {
    let Accounts::Confirm(target) = &s.settings_pages().accounts else { return vec![] };
    let (title, description, action) = match target {
        Removal::Provider { name, description, .. } => (t!("settings.providers.disconnect.title", provider = name), description.clone(), t!("common.disconnect")),
        Removal::Account { label, provider, last, .. } => {
            let description = if *last {
                t!("settings.providers.account.remove.finalDescription", provider = provider)
            } else {
                t!("settings.providers.account.remove.description", provider = provider)
            };
            (t!("settings.providers.account.remove.title", account = label), description, t!("dialog.provider.accounts.remove"))
        }
    };
    let cancel = |s: &mut H, tx: &mut Tx| step_accounts(s, Event::Cancel, tx);
    let inner = col()
        .w_full()
        .clip_rounded(12.0)
        .child(col().padding(16.0).gap(4.0).child(text(title.clone()).font(DIALOG_TITLE).color(p.text).wrap()).child(text(description).font(BODY).color(p.muted).wrap()))
        .child(
            row()
                .padding(16.0)
                .gap(8.0)
                .child(spacer())
                .child(button(t!("common.cancel"), ButtonVariant::Ghost).on_press_tx(cancel).build(kit(p)).autofocus())
                .child(button(action, ButtonVariant::Danger).on_press_tx(|s: &mut H, tx| step_accounts(s, Event::Submit, tx)).build(kit(p))),
        );
    let card = widgets::overlay(p, col())
        .key("provider-removal")
        .role("alertdialog")
        .label(title)
        .radius(12.0)
        .fill(p.layer1)
        .modal_tx(true, cancel)
        .child(inner);
    sized_modal("provider-removal-modal", p, DialogSize::Normal, Fit::Content, dialog_window(cx, s.zoom()), card)
}

fn source(i: Option<&Integration>) -> &'static str {
    let Some(i) = i else { return t!("settings.providers.tag.other") };
    match i.connections.iter().find(|c| c.credential) {
        Some(c) if c.method == "key" => t!("provider.connect.method.apiKey"),
        Some(_) => t!("settings.providers.tag.account"),
        None if i.connections.iter().any(|c| !c.credential) => t!("settings.providers.tag.environment"),
        None => t!("settings.providers.tag.config"),
    }
}

pub fn view<H: SettingsHost + PopupHost + 'static>(s: &H, p: &Pal) -> El {
    let integrations = s.integrations(None).unwrap_or_default();
    let integration = |pi: &ProviderRow| integrations.iter().find(|i| i.id == pi.integration.as_deref().unwrap_or(&pi.id));
    let console = integrations.iter().find(|i| i.id == "opencode");
    let console_connected = console.is_some_and(|i| !i.connections.is_empty());
    let paid = |id: &str| s.models().iter().any(|m| m.provider == id && !m.free);
    let provider_rows = s.provider_rows(None);
    let managed = super::models::console_group(s);
    let mut connected: Vec<&ProviderRow> = provider_rows
        .iter()
        .filter(|pi| pi.id != "opencode" || managed.is_some() || console_connected || paid(&pi.id))
        .collect();
    connected.sort_by_key(|pi| pi.id != "opencode-go");
    let displayed: Vec<&ProviderRow> = match &managed {
        Some(cg) => connected.iter().copied().filter(|pi| pi.id == cg.root_id || !cg.ids.contains(&pi.id)).collect(),
        None => connected.clone(),
    };
    let rows: Vec<El> = displayed
        .iter()
        .map(|pi| match &managed {
            Some(cg) if cg.root_id == pi.id => console_row(s, p, pi, cg, integration(pi)),
            _ => connected_row(s, p, pi, integration(pi)),
        })
        .collect();
    let connected_list = if rows.is_empty() {
        catalog(p, "connected-providers", vec![el().py(16.0).child(text(t!("settings.providers.connected.empty")).font(super::NAME_REGULAR).color(p.muted))])
    } else {
        catalog(p, "connected-providers", rows)
    };
    let ids: Vec<&str> = connected.iter().map(|pi| pi.id.as_str()).collect();
    let mut popular: Vec<(String, String)> = integrations
        .iter()
        .filter(|i| POPULAR.contains(&i.id.as_str()))
        // OpenCode Go connects through OpenCode, so it is listed only for someone who has saved a Go
        // key (`providers.tsx` `popular`, opencode#65740).
        .filter(|i| i.id != "opencode-go" || i.connections.iter().any(|c| c.credential))
        .filter(|i| {
            if i.id == "opencode" {
                i.connections.iter().find(|c| c.credential).is_none_or(|c| c.method != "oauth")
            } else {
                !ids.contains(&i.id.as_str())
            }
        })
        .map(|i| (i.id.clone(), i.name.clone()))
        .collect();
    popular.sort_by_key(|(id, _)| POPULAR.iter().position(|p| p == id));
    let more_words = t!("dialog.provider.viewAll");
    let more = text(more_words)
        .key("settings-providers-view-all")
        .font(NAME)
        .color(p.accent)
        // Inside `.settings-section` (`gap: 16px`) plus `.settings-providers-view-all { margin-top: 20px }` (`settings.css`).
        .mt(36.0)
        .link(Link::new(0..more_words.len(), "more").role("button").label(more_words).hover(LinkPaint::new().color(p.accent_hover)))
        // `.settings-providers-view-all`: the arrow on Desktop (`packages/desktop/src/renderer/styles.css:15`).
        .cursor(rune::Cursor::Default)
        .on_link(|s: &mut H, _: &str, tx: &mut Tx| open_connect_from_settings(s, None, tx));
    let body = col()
        .key("settings-providers")
        .w_full()
        .gap(32.0)
        .mt(8.0)
        .child(section(p, "connected-providers-section", Some(t!("settings.providers.section.connected")), connected_list))
        .child(
            col()
                .w_full()
                .child(section(
                    p,
                    "popular-providers-section",
                    Some(t!("settings.providers.section.popular")),
                    catalog(p, "popular-providers", popular.iter().map(|(id, name)| popular_row::<H>(p, id, name)).collect()),
                ))
                .child(more),
        );
    page(p, "settings-providers-page", header_with(p, t!("settings.providers.title"), t!("settings.providers.description"), super::servers::switcher(s, p)), body)
}

fn toggle_console_providers(s: &mut dyn SettingsHost, tx: &mut Tx) {
    tx.instant();
    let pages = s.settings_pages_mut();
    pages.console_expanded = !pages.console_expanded;
}

/// Connected OpenCode Console workspace row (`providers.tsx:551-612`, `settings.css:1079-1200`):
/// `OpenCode` + workspace badge, expandable provider list where clicking any provider jumps to
/// Settings › Models, expands its section, and scrolls/focuses it.
fn console_row<H: SettingsHost + PopupHost + 'static>(
    s: &H,
    p: &Pal,
    pi: &ProviderRow,
    cg: &super::models::ConsoleGroup,
    integration: Option<&Integration>,
) -> El {
    let expanded = s.settings_pages().console_expanded;
    let display = t!("provider.connect.opencode.name");
    let toggle = (cg.providers.len() > 1).then(|| {
        let count_label = opencode_i18n::plural!("settings.providers.console.available", cg.providers.len());
        row()
            .key("settings-provider-console-toggle")
            .role("button")
            .label(count_label.clone())
            .expanded(expanded)
            .h(28.0)
            .px(4.0)
            .gap(2.0)
            .center_y()
            .radius(4.0)
            .cursor(Cursor::Pointer)
            .focus_default(p)
            .hover_group_focus()
            .hover(p.hover)
            .on_press_tx(|s: &mut H, tx: &mut Tx| toggle_console_providers(s, tx))
            .child(text(count_label).font(super::NAME_REGULAR).color(p.muted).group_color(p.text))
            .child(
                icon(&ic::CHEVRON_DOWN, 14.0)
                    .color(p.icon_muted)
                    .rotate(if expanded { 0.0 } else { -std::f32::consts::FRAC_PI_2 })
                    .mirror_in_rtl(),
            )
    });
    let summary = row()
        .gap(4.0)
        .shrink()
        .min_w(0.0)
        .wrap()
        .center_y()
        .child(
            row()
                .gap(8.0)
                .shrink()
                .min_w(0.0)
                .center_y()
                .child(text(display).font(NAME).color(p.text).truncate())
                .child(badge(p, &cg.workspace)),
        )
        .children(toggle);
    let lead = row()
        .grow(1.0)
        .shrink()
        .min_w(0.0)
        .gap(8.0)
        .center_y()
        .child(widgets::provider_mark("opencode", if p.dark { Color::WHITE } else { Color::BLACK }))
        .child(summary);
    let accounts = integration.map(Integration::accounts).unwrap_or_default();
    let id = pi.integration.clone().unwrap_or_else(|| pi.id.clone());
    let trailing = if !accounts.is_empty() {
        account_menu(s, p, &pi.id, &id, display, integration)
    } else {
        let id_owned = id.clone();
        button(t!("common.disconnect"), ButtonVariant::GhostMuted)
            .on_press_tx(move |s: &mut H, tx: &mut Tx| ask_disconnect_provider(s, &id_owned, display, tx))
            .build(kit(p))
    };
    let mut header = row()
        .key("settings-provider-console-header")
        .w_full()
        .min_h(48.0)
        .py(10.0)
        .gap(16.0)
        .center_y()
        .child(lead)
        .child(trailing);
    if crate::rows_wrap() {
        header = header.wrap();
    }
    let mut block = col().key(format!("settings-provider-{}", pi.id)).w_full().child(header);
    if expanded {
        let mut list = col()
            .key("settings-provider-console-list")
            .w_full()
            .gap(2.0)
            .pb(10.0)
            .child(el().w_full().h(0.5).mb(6.0).fill(p.border));
        for prov in &cg.providers {
            let label = prov.name.strip_prefix(&cg.prefix).unwrap_or(&prov.name).to_owned();
            let pid = prov.id.clone();
            list = list.child(
                row()
                    .key(format!("settings-provider-console-item-{}", prov.id))
                    .role("button")
                    .label(label.clone())
                    .w_full()
                    .min_h(24.0)
                    .px(10.0)
                    .gap(2.0)
                    .center_y()
                    .radius(4.0)
                    .cursor(Cursor::Pointer)
                    .focus_default(p)
                    .hover(p.hover)
                    .hover_group_focus()
                    .on_press_tx(move |s: &mut H, tx: &mut Tx| super::models::open_for_provider(s, &pid, tx))
                    .child(text(label).font(super::NAME_REGULAR).color(p.text).truncate())
                    .child(
                        icon(&ic::CHEVRON_DOWN, 14.0)
                            .color(p.icon)
                            .rotate(-std::f32::consts::FRAC_PI_2)
                            .mirror_in_rtl()
                            .group_opacity(0.0, 1.0),
                    ),
            );
        }
        block = block.child(list);
    }
    block
}

fn connected_row<H: SettingsHost + PopupHost + 'static>(s: &H, p: &Pal, pi: &ProviderRow, integration: Option<&Integration>) -> El {
    let lead = row()
        .grow(1.0)
        .shrink()
        .min_w(0.0)
        .gap(8.0)
        .center_y()
        .child(widgets::provider_mark(&pi.id, if p.dark { Color::WHITE } else { Color::BLACK }))
        .child(
            row()
                .gap(8.0)
                .shrink()
                .min_w(0.0)
                .center_y()
                .child(text(pi.name.clone()).font(NAME).color(p.text).truncate().help_when_truncated(pi.name.clone()).help_above())
                .child(badge(p, source(integration))),
        );
    let accounts = integration.map(Integration::accounts).unwrap_or_default();
    let can_disconnect = integration.is_some_and(|i| i.connections.iter().any(|c| c.credential));
    let id = pi.integration.clone().unwrap_or_else(|| pi.id.clone());
    let trailing = if !accounts.is_empty() {
        let mut e = row().gap(0.0).center_y();
        if let Some(status) = accounts.first().and_then(|a| a.status.as_ref()) {
            let (again, warning) = (id.clone(), p.warning);
            let mut btn = button(t!("dialog.provider.needsSignIn"), ButtonVariant::GhostMuted)
                .label_with(move |font, _| text(t!("dialog.provider.needsSignIn")).font(font).color(warning))
                .on_press_tx(move |s: &mut H, tx: &mut Tx| open_connect_from_settings(s, Some(&again), tx))
                .build(kit(p))
                .key("settings-provider-sign-in");
            if !status.message.is_empty() {
                btn = btn.help(status.message.clone()).help_above();
            }
            e = e.child(btn);
        }
        e.child(account_menu(s, p, &pi.id, &id, &pi.name, integration))
    } else if can_disconnect {
        let (id, name) = (id.clone(), pi.name.clone());
        button(t!("common.disconnect"), ButtonVariant::GhostMuted)
            .on_press_tx(move |s: &mut H, tx: &mut Tx| ask_disconnect_provider(s, &id, &name, tx))
            .build(kit(p))
    } else {
        text(t!("settings.providers.connected.environmentDescription"))
            .key("settings-provider-env-hint")
            .font(super::NAME_REGULAR)
            .color(p.muted)
            .pr(12.0)
            .group_opacity(0.0, 1.0)
    };
    row()
        .key(format!("settings-provider-{}", pi.id))
        .w_full()
        .min_h(48.0)
        .py(10.0)
        .gap(16.0)
        .center_y()
        .hover_group_focus()
        .child(lead)
        .child(trailing)
}

fn popular_row<H: SettingsHost + 'static>(p: &Pal, id: &str, name: &str) -> El {
    let mut main = row().gap(8.0).center_y().child(text(name.to_owned()).font(NAME).color(p.text));
    if id == "opencode" || id == "opencode-go" {
        main = main.child(badge(p, t!("dialog.provider.tag.recommended")));
    }
    let copy = col().grow(1.0).shrink().min_w(0.0).gap(6.0).child(main).children(note(id).map(|n| text(n).font(BODY).color(p.muted).wrap()));
    let open = id.to_owned();
    row()
        .key(format!("settings-popular-{id}"))
        .w_full()
        .py(16.0)
        .gap(16.0)
        .center_y()
        .child(
            row()
                .grow(1.0)
                .shrink()
                .min_w(0.0)
                .gap(8.0)
                .child(widgets::provider_mark(id, if id == "opencode" { if p.dark { Color::WHITE } else { Color::BLACK } } else { p.icon }))
                .child(copy),
        )
        .child(
            button(t!("common.connect"), ButtonVariant::Neutral)
                .icon(&ic::PLUS)
                .on_press_tx(move |s: &mut H, tx: &mut Tx| open_connect_from_settings(s, Some(&open), tx))
                .build(kit(p))
                .key(format!("settings-connect-{id}")),
        )
}

fn account_menu<H: SettingsHost + PopupHost + 'static>(s: &H, p: &Pal, provider: &str, id: &str, name: &str, integration: Option<&Integration>) -> El {
    let accounts = integration.map(Integration::accounts).unwrap_or_default();
    let active = accounts.first().map(|a| (a.id.clone(), a.label.clone()));
    if let (Accounts::Renaming { provider: row, credential, field }, Some((active_id, _))) = (&s.settings_pages().accounts, &active)
        && row == provider
        && credential == active_id
    {
        return rune_kit::TextInput::new(field)
            .label(t!("settings.providers.account.name"))
            .width(Some(240.0))
            .spellcheck(s.settings().spellcheck)
            .dir(rune::Direction::Auto)
            .on_edit(|s: &mut H| s.settings_pages_mut().account_rename_field_mut())
            .on_submit(|s: &mut H, tx: &mut Tx| step_accounts(s, Event::Submit, tx))
            .on_key(|s: &mut H, ev: &KeyEvent, tx: &mut Tx| rename_key(s, ev, tx))
            .build(kit(p));
    }
    let label = active.as_ref().map(|a| a.1.clone()).unwrap_or_default();
    let accounts_anchor = format!("settings-provider-accounts-{provider}");
    let options_anchor = format!("settings-provider-options-{provider}");
    let accounts_open = s.popup().open_at(&accounts_anchor);
    let options_open = s.popup().open_at(&options_anchor);
    let account_list: Vec<(String, String)> = accounts.iter().map(|a| (a.id.clone(), a.label.clone())).collect();
    let add_id = id.to_owned();
    let provider_name = name.to_owned();
    let accounts_anchor_click = accounts_anchor.clone();
    let trigger = button(label.clone(), ButtonVariant::GhostMuted)
        .label_with(move |font, ink| row().gap(6.0).center_y().child(text(label.clone()).font(font).color(ink).truncate()).child(icon(&ic::CHEVRON_DOWN, 16.0).color(ink)))
        .expanded(accounts_open)
        .build(kit(p))
        .key(accounts_anchor.clone())
        .menu_button(accounts_anchor, move |s: &mut H, tx: &mut Tx| {
            let items = account_list.iter().enumerate().map(|(at, (cid, clabel))| Item::new(format!("acct:{cid}"), clabel.clone()).checked(at == 0));
            let menu = PopupMenu::new(t!("settings.providers.account.group"))
                .align(Align::End)
                .gap(6.0)
                .min_width(220.0)
                .group(t!("settings.providers.account.group"))
                .items(items)
                .separator()
                .item(Item::new("add", t!("settings.providers.account.add")).slot_icon(&ic::PLUS));
            let (account_list, add_id, provider_name) = (account_list.clone(), add_id.clone(), provider_name.clone());
            popup::open(s, menu, accounts_anchor_click.clone(), tx, move |s: &mut H, picked, tx| match picked {
                Some("add") => add_account(s, &add_id, tx),
                Some(raw) => {
                    if let Some(cid) = raw.strip_prefix("acct:")
                        && let Some((_, clabel)) = account_list.iter().find(|(id, _)| id == cid)
                    {
                        activate_account(s, cid, &provider_name, clabel, tx);
                    }
                }
                None => {}
            });
        });
    let options_label = t!("dialog.provider.accounts.options", account = active.as_ref().map(|a| a.1.as_str()).unwrap_or_default());
    let (opt_provider, opt_name, opt_active, opt_last) = (provider.to_owned(), name.to_owned(), active.clone(), accounts.len() == 1);
    let options_anchor_click = options_anchor.clone();
    let dots = el()
        .key(options_anchor.clone())
        .role("button")
        .label(options_label)
        .size(32.0, 32.0)
        .radius(6.0)
        .center()
        .focus_default(p)
        .fill(if options_open { p.pressed } else { Color::TRANSPARENT })
        .hover(if options_open { p.pressed } else { p.hover })
        .press(p.pressed)
        .menu_button(options_anchor, move |s: &mut H, tx: &mut Tx| {
            let Some((cid, clabel)) = opt_active.clone() else { return };
            let menu = PopupMenu::new(t!("common.moreOptions"))
                .align(Align::End)
                .gap(6.0)
                .item(Item::new("rename", t!("common.rename")))
                .item(Item::new("remove", t!("dialog.provider.accounts.remove")));
            let (opt_provider, opt_name) = (opt_provider.clone(), opt_name.clone());
            popup::open(s, menu, options_anchor_click.clone(), tx, move |s: &mut H, picked, tx| match picked {
                Some("rename") => step_accounts(s, Event::Rename { provider: opt_provider.clone(), credential: cid.clone(), label: clabel.clone() }, tx),
                Some("remove") => step_accounts(
                    s,
                    Event::Ask(Removal::Account { credential: cid.clone(), provider: opt_name.clone(), label: clabel.clone(), last: opt_last }),
                    tx,
                ),
                _ => {}
            });
        })
        .child(icon(&ic::OUTLINE_DOTS, 16.0).color(p.icon_muted));
    row().key("settings-provider-account-actions").gap(0.0).center_y().child(trigger).child(dots)
}

fn require_writable(s: &mut dyn SettingsHost) -> bool {
    if !s.can_write() {
        s.toast_fail(t!("common.requestFailed").into(), t!("rune.jobs.readOnly").into());
        return false;
    }
    true
}

fn disconnect_provider(s: &mut dyn SettingsHost, id: &str, name: &str) {
    if !require_writable(s) {
        return;
    }
    s.spawn_job(SettingsJob::ActiveServer(SettingsRequest::DisconnectProvider {
        id: id.to_owned(),
        name: name.to_owned(),
    }));
}

fn activate_account(s: &mut dyn SettingsHost, credential: &str, provider: &str, label: &str, tx: &mut Tx) {
    tx.instant();
    let _ = s.settings_pages_mut().accounts.step(Event::Close);
    if !require_writable(s) {
        return;
    }
    s.spawn_job(SettingsJob::ActiveServer(SettingsRequest::ActivateAccount {
        credential: credential.to_owned(),
        provider: provider.to_owned(),
        label: label.to_owned(),
    }));
}

fn remove_account(s: &mut dyn SettingsHost, credential: &str, provider: &str, tx: &mut Tx) {
    tx.instant();
    let _ = s.settings_pages_mut().accounts.step(Event::Close);
    if !require_writable(s) {
        return;
    }
    let label = s
        .integrations(None)
        .unwrap_or_default()
        .iter()
        .flat_map(|i| i.connections.iter())
        .find(|c| c.id == credential)
        .map(|c| c.label.clone())
        .unwrap_or_default();
    s.spawn_job(SettingsJob::ActiveServer(SettingsRequest::RemoveAccount {
        credential: credential.to_owned(),
        provider: provider.to_owned(),
        label,
    }));
}

fn rename_account(s: &mut dyn SettingsHost, credential: String, label: String) {
    if !require_writable(s) {
        return;
    }
    s.spawn_job(SettingsJob::ActiveServer(SettingsRequest::RenameAccount { credential, label }));
}
