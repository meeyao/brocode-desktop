//! Settings › Extensions (`packages/app/src/settings/providers/extensions.tsx`): the server's
//! MCPs (with OpenCode Console manage/connect links, override badges, status dots, and switches),
//! package/local plugins (with explicit Check for updates / per-plugin Update controls that hide
//! quietly on older servers), and skills (with Console links for virtual `skill://` skills),
//! under pill tabs (`anomalyco/opencode#63125`, `#64511`, `#64525`, `#64527`).

use std::collections::HashSet;

use opencode_i18n::t;
use opencode_kit::integration::{McpServer, McpStatus};
use opencode_kit::model::{PluginInfo, SkillInfo};
use opencode_kit::servers::ServerKey;
use opencode_ui::focus::FocusExt;
use opencode_ui::palette::{Pal, kit};
use opencode_ui::popup::PopupHost;
use opencode_ui::widgets;
use rune::{El, TextStyle, Tx, col, icon, row, rule, text};
use rune_kit::{ButtonVariant, ControlSize};
use rune_theme_opencode::icons as ic;

use super::console_link::{ConsoleSources, console_connect_button, console_name, console_replaced, console_signed_in, load_failed};
use super::plugin_update::{self, Effect as UpdateEffect, Event as UpdateEvent, State as UpdateState};
use super::{BODY, ExtensionsTab, LINE, NAME, Page, catalog_with, header_with, page};
use crate::host::SettingsHost;
use crate::jobs::{SettingsJob, SettingsRequest};

/// A tab's label: 13 on 16 at 440.
const TAB: TextStyle = TextStyle::new(13.0, 440.0).lh(16.0).tracking(-0.04);

/// Ephemeral state held by Settings › Extensions for the selected server.
pub struct State {
    pub server: ServerKey,
    pub active_dir: Option<String>,
    pub plugins: Option<Result<Vec<PluginInfo>, ()>>,
    pub skills: Option<Vec<SkillInfo>>,
    pub sources: ConsoleSources,
    pub updates: UpdateState,
    /// Servers whose `/api/plugin/{check,update}` answered 404/405/501; cleared on reconnect.
    pub unsupported_update_servers: HashSet<ServerKey>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            server: ServerKey::sidecar(),
            active_dir: None,
            plugins: None,
            skills: None,
            sources: ConsoleSources::default(),
            updates: UpdateState::default(),
            unsupported_update_servers: HashSet::new(),
        }
    }
}

/// Reads the server's plugins, skills, MCPs, integrations, and Console extension sources.
pub fn load(s: &mut dyn SettingsHost) {
    let key = s.settings_pages().server.key.clone();
    let active_dir = s.active_session_directory();
    let st = &mut s.settings_pages_mut().extensions;
    if st.server != key {
        st.server = key.clone();
        st.plugins = None;
        st.skills = None;
        st.sources = ConsoleSources::default();
        let unsupported = st.unsupported_update_servers.contains(&key);
        let _ = plugin_update::step(&mut st.updates, UpdateEvent::ServerChanged { unsupported });
    }
    st.active_dir = active_dir.clone();
    s.sync_mcp("");
    if let Some(dir) = active_dir.as_deref().filter(|d| !d.is_empty()) {
        s.sync_mcp(dir);
    }
    s.sync_catalogs(active_dir.as_deref());
    s.spawn_job(SettingsJob::ActiveServer(SettingsRequest::LoadExtensions {
        server: key,
        active_dir,
    }));
}

/// Steps the plugin-update state machine and executes its returned side effects.
pub fn dispatch_update(s: &mut dyn SettingsHost, event: UpdateEvent, _tx: &mut Tx) {
    let server = s.settings_pages().extensions.server.clone();
    let effects = plugin_update::step(&mut s.settings_pages_mut().extensions.updates, event);
    for effect in effects {
        match effect {
            UpdateEffect::Check { attempt } => {
                s.spawn_job(SettingsJob::ActiveServer(SettingsRequest::CheckPluginUpdates {
                    server: server.clone(),
                    attempt,
                }));
            }
            UpdateEffect::Update { id, target } => {
                s.spawn_job(SettingsJob::ActiveServer(SettingsRequest::UpdatePlugin {
                    server: server.clone(),
                    id,
                    target,
                }));
            }
            UpdateEffect::Refresh => {
                s.spawn_job(SettingsJob::ActiveServer(SettingsRequest::ReloadPlugins {
                    server: server.clone(),
                }));
            }
            UpdateEffect::Failed { message } => {
                s.toast_fail(t!("common.requestFailed").into(), message);
            }
            UpdateEffect::MarkServerUnsupported => {
                s.settings_pages_mut().extensions.unsupported_update_servers.insert(server.clone());
            }
        }
    }
}

/// Called on `plugin.updated` SSE: refreshes the plugin list when Settings › Extensions is open.
pub fn on_plugin_updated(s: &mut dyn SettingsHost) {
    if !s.settings_open() || s.settings_pages().page != Page::Extensions {
        return;
    }
    let server = s.settings_pages().extensions.server.clone();
    s.spawn_job(SettingsJob::ActiveServer(SettingsRequest::ReloadPlugins { server }));
}

/// Called on `config.updated` SSE: refreshes Console extension sources when Extensions is open.
pub fn on_config_updated(s: &mut dyn SettingsHost) {
    if !s.settings_open() || s.settings_pages().page != Page::Extensions {
        return;
    }
    let st = &s.settings_pages().extensions;
    let (server, active_dir) = (st.server.clone(), st.active_dir.clone());
    s.spawn_job(SettingsJob::ActiveServer(SettingsRequest::ReloadConsoleSources { server, active_dir }));
}

/// Clears cached `Unsupported` update state for `server` on reconnect so an upgraded server can
/// advertise plugin updates again without restarting the app.
pub fn on_server_reconnected(s: &mut dyn SettingsHost, server: &ServerKey) {
    let st = &mut s.settings_pages_mut().extensions;
    let removed = st.unsupported_update_servers.remove(server);
    if removed && st.server == *server {
        let _ = plugin_update::step(&mut st.updates, UpdateEvent::ServerChanged { unsupported: false });
    }
    if s.settings_open() && s.settings_pages().page == Page::Extensions && s.settings_pages().extensions.server == *server {
        load(s);
    }
}

fn select_tab(s: &mut dyn SettingsHost, tab: ExtensionsTab, tx: &mut Tx) {
    tx.instant();
    s.settings_pages_mut().tab = tab;
}

fn retry_sources(s: &mut dyn SettingsHost, _tx: &mut Tx) {
    let st = &s.settings_pages().extensions;
    let (server, active_dir) = (st.server.clone(), st.active_dir.clone());
    s.spawn_job(SettingsJob::ActiveServer(SettingsRequest::ReloadConsoleSources { server, active_dir }));
}

fn retry_plugins(s: &mut dyn SettingsHost, _tx: &mut Tx) {
    let server = s.settings_pages().extensions.server.clone();
    s.settings_pages_mut().extensions.plugins = None;
    s.spawn_job(SettingsJob::ActiveServer(SettingsRequest::ReloadPlugins { server }));
}

pub fn view<H: SettingsHost + PopupHost + 'static>(s: &H, p: &Pal) -> El {
    let tab = s.settings_pages().tab;
    let (link, content) = match tab {
        ExtensionsTab::Mcps => (manage(p), mcps::<H>(s, p)),
        ExtensionsTab::Plugins => (plugins_header_actions::<H>(s, p), plugins_body::<H>(s, p)),
        ExtensionsTab::Skills => (skills_link::<H>(p), skills_body::<H>(s, p)),
    };
    let heading = row()
        .w_full()
        .min_h(24.0)
        .gap(8.0)
        .center_y()
        .wrap()
        .child(text(t!("settings.extensions.availableAll")).font(LINE).color(p.text).grow(1.0).shrink().min_w(0.0))
        .child(link);
    let body = col()
        .key("settings-extensions")
        .w_full()
        .child(tabs::<H>(p, tab))
        .child(col().w_full().pt(24.0).gap(16.0).child(heading).children(content))
        .children((tab == ExtensionsTab::Plugins).then(|| s.plugin_extensions_section(p)).flatten().map(|e| e.mt(32.0)));
    page(p, "settings-extensions-page", header_with(p, t!("settings.tab.extensions"), t!("settings.extensions.description"), super::servers::switcher(s, p)), body)
}

/// Pill tabs (`tabs-v2` pill in `.settings-subtabs`).
fn tabs<H: SettingsHost + 'static>(p: &Pal, on: ExtensionsTab) -> El {
    let mut list = row().key("settings-extensions-tabs").role("tablist").focus_group().w_full().max_w(280.0).gap(6.0).pb(8.0);
    for (tab, id, label) in [
        (ExtensionsTab::Mcps, "mcps", t!("settings.extensions.tab.mcps")),
        (ExtensionsTab::Plugins, "plugins", t!("settings.extensions.tab.plugins")),
        (ExtensionsTab::Skills, "skills", t!("settings.extensions.tab.skills")),
    ] {
        let picked = tab == on;
        let edge = if picked { p.border_muted } else { rune::Color::TRANSPARENT };
        let label_el = text(label).font(TAB).color(if picked { p.text } else { p.muted });
        let label_el = if picked { label_el } else { label_el.group_color(p.text) };
        let mut pill = row()
            .key(format!("settings-extensions-tab-{id}"))
            .role("tab")
            .label(label)
            .focusable(true)
            .focus_selected(picked)
            .focus_default(p)
            .hover_group_focus()
            .h(24.0)
            .center()
            .radius(6.0)
            .border(0.5, edge)
            .border_layout()
            .on_press_tx(move |s: &mut H, tx: &mut Tx| select_tab(s, tab, tx))
            .child(row().px(8.0).border(0.5, rune::Color::TRANSPARENT).border_layout().center().child(label_el));
        pill = if picked { pill.fill(p.layer3) } else { pill.hover(p.layer3) };
        list = list.child(pill);
    }
    list
}

fn manage(p: &Pal) -> El {
    text(t!("settings.extensions.manageConfig")).font(LINE).color(p.muted)
}

fn plugins_header_actions<H: SettingsHost + 'static>(s: &H, p: &Pal) -> El {
    let st = &s.settings_pages().extensions;
    let has_packages = st
        .plugins
        .as_ref()
        .and_then(|r| r.as_ref().ok())
        .is_some_and(|list| list.iter().any(|pl| pl.package_source().is_some()));
    let check_btn = (has_packages && plugin_update::supported(&st.updates)).then(|| {
        let checking = plugin_update::checking(&st.updates);
        let label = if checking { t!("settings.plugins.checking") } else { t!("settings.plugins.check") };
        widgets::button(label, ButtonVariant::GhostMuted)
            .control_size(ControlSize::Regular)
            .disabled(checking)
            .on_press_tx(|s: &mut H, tx: &mut Tx| dispatch_update(s, UpdateEvent::CheckRequested, tx))
            .build(kit(p))
            .key("settings-plugins-check")
            .busy(checking)
    });
    row().gap(12.0).center_y().child(manage(p)).children(check_btn)
}

/// `.settings-extension-link`: underlined in `text-accent`, `text-accent-hover` under the pointer.
fn skills_link<H: SettingsHost + 'static>(p: &Pal) -> El {
    let words = t!("settings.extensions.addSkills");
    const URL: &str = "https://github.com/meeyao/brocode-desktop";
    text(words)
        .key("settings-extension-link")
        .font(LINE)
        .color(p.accent)
        .spans(vec![rune::Span::new(0, words.len()).underline(p.accent)])
        .link(rune::Link::new(0..words.len(), URL).role("link").label(words).hover(rune::LinkPaint::new().color(p.accent_hover).underline(p.accent_hover)))
        .on_link(|s: &mut H, url: &str, tx: &mut Tx| crate::open_external(s, url, tx))
}

/// `.settings-extension-row`: 56 pt tall with its rule. The last row has no rule.
fn extension_row(p: &Pal, key: String, lead: El, name: El, badge: Option<El>, trailing: Option<El>, last: bool) -> El {
    let r = row()
        .key(key)
        .w_full()
        .min_h(56.0)
        .py(16.0)
        .gap(16.0)
        .center_y()
        .child(row().grow(1.0).shrink().min_w(0.0).gap(8.0).center_y().child(lead).child(name).children(badge))
        .children(trailing);
    if last { r } else { r.child(rule(0.5, p.border).pin(Some(0.0), None, Some(0.0), Some(0.0))) }
}

fn mcps<H: SettingsHost + 'static>(s: &H, p: &Pal) -> Vec<El> {
    let st = &s.settings_pages().extensions;
    let dir = st.active_dir.as_deref().unwrap_or("");
    let active = (!dir.is_empty()).then(|| s.mcp_servers(dir)).flatten();
    let mut list: Vec<McpServer> = s
        .mcp_servers("")
        .unwrap_or_default()
        .iter()
        .map(|srv| {
            active
                .and_then(|a| a.iter().find(|item| item.name == srv.name))
                .cloned()
                .unwrap_or_else(|| srv.clone())
        })
        .collect();
    list.sort_by(|a, b| rune::text::compare_names(&a.name, &b.name));
    let n = list.len();
    let rows: Vec<El> = list
        .iter()
        .enumerate()
        .map(|(k, m)| {
            let disabled = m.status == McpStatus::Disabled;
            let managed = st.sources.managed(&m.name);
            let links = st.sources.links(&m.name);
            let dot_status = if managed.is_some_and(|c| c.needs_connection) && m.status == McpStatus::Connected {
                "needs_auth"
            } else {
                m.status.name()
            };
            let name_el = match (managed, &links) {
                (Some(_), Some(l)) => console_name::<H>(p, format!("settings-mcp-console-{}", m.name), &m.name, l.manage.clone(), disabled),
                _ => text(m.name.clone())
                    .font(NAME)
                    .color(if disabled { p.muted } else { p.text })
                    .shrink()
                    .truncate()
                    .help_when_truncated(m.name.clone())
                    .help_above(),
            };
            let badge = st
                .sources
                .mcp(&m.name)
                .filter(|c| c.overridden)
                .and(links.as_ref())
                .map(|l| console_replaced::<H>(p, format!("settings-mcp-replaced-{}", m.name), &m.name, l.manage.clone()));
            let trailing = match st.sources.connect(&m.name) {
                Some(connect_url) => console_connect_button::<H>(p, format!("settings-mcp-connect-{}", m.name), &m.name, connect_url),
                None => {
                    let ctl = s.mcp_row_trailing(p, dir, m);
                    if managed.is_some() {
                        row()
                            .label(format!("{}: {}", m.name, t!("settings.extensions.console.temporary")))
                            .center_y()
                            .child(ctl)
                    } else {
                        ctl
                    }
                }
            };
            extension_row(
                p,
                format!("settings-mcp-{}", m.name),
                widgets::status_dot(p, dot_status),
                name_el,
                badge,
                Some(trailing),
                k + 1 == n,
            )
        })
        .collect();
    let mut out = Vec::new();
    if !rows.is_empty() {
        out.push(catalog_with(p, "settings-mcps", rows, false));
    }
    if st.sources.failed() {
        out.push(load_failed::<H>(
            p,
            "settings-extensions-sources-failed",
            t!("settings.extensions.sources.loadFailed"),
            t!("common.retry"),
            retry_sources,
        ));
    }
    out
}

fn plugins_body<H: SettingsHost + 'static>(s: &H, p: &Pal) -> Vec<El> {
    let st = &s.settings_pages().extensions;
    let mut out = Vec::new();
    match &st.plugins {
        None => {
            if s.plugin_extensions_section(p).is_none() {
                out.push(text(t!("common.loading")).font(BODY).color(p.muted));
            }
        }
        Some(Err(())) => {
            out.push(load_failed::<H>(
                p,
                "settings-extensions-plugins-failed",
                t!("settings.extensions.plugins.loadFailed"),
                t!("common.retry"),
                retry_plugins,
            ));
        }
        Some(Ok(list)) => {
            let listed: Vec<&PluginInfo> = list.iter().filter(|pl| !pl.is_builtin()).collect();
            let total = listed.len();
            let can_update = plugin_update::supported(&st.updates);
            let rows: Vec<El> = listed
                .iter()
                .enumerate()
                .map(|(k, pl)| {
                    let label = pl.label();
                    let trailing = can_update.then(|| {
                        let (target, outdated, updating) = pl.package_source()?;
                        let v = plugin_update::view(&st.updates, target, outdated, updating);
                        (v.outdated || v.updating).then(|| {
                            let target_owned = target.to_owned();
                            let btn_text = if v.updating { t!("settings.plugins.updating") } else { t!("settings.plugins.update") };
                            widgets::button(btn_text, ButtonVariant::GhostMuted)
                                .control_size(ControlSize::Regular)
                                .disabled(v.updating)
                                .on_press_tx(move |s: &mut H, tx: &mut Tx| {
                                    dispatch_update(s, UpdateEvent::UpdateRequested { target: target_owned.clone() }, tx);
                                })
                                .build(kit(p))
                                .key(format!("settings-plugin-update-{target}"))
                                .label(t!("settings.plugins.update.label", name = label))
                                .busy(v.updating)
                        })
                    })
                    .flatten();
                    extension_row(
                        p,
                        format!("settings-plugins-{label}"),
                        icon(&ic::CUBE, 16.0).color(p.icon_muted),
                        text(label).font(NAME).color(p.text).shrink().truncate(),
                        None,
                        trailing,
                        k + 1 == total,
                    )
                })
                .collect();
            if !rows.is_empty() {
                out.push(catalog_with(p, "settings-plugins", rows, false));
            }
        }
    }
    out
}

fn skills_body<H: SettingsHost + 'static>(s: &H, p: &Pal) -> Vec<El> {
    let st = &s.settings_pages().extensions;
    let signed_in = console_signed_in(s, st.active_dir.as_deref());
    let mut skills: Vec<&SkillInfo> = st.skills.as_deref().unwrap_or_default().iter().collect();
    skills.sort_by(|a, b| rune::text::compare_names(&a.name, &b.name));
    let total = skills.len();
    let rows: Vec<El> = skills
        .iter()
        .enumerate()
        .map(|(k, sk)| {
            let name_el = match st.sources.skill(sk, signed_in) {
                Some(href) => console_name::<H>(p, format!("settings-skill-console-{}", sk.name), &sk.name, href, false),
                None => text(sk.name.clone())
                    .font(NAME)
                    .color(p.text)
                    .shrink()
                    .truncate()
                    .help_when_truncated(sk.name.clone())
                    .help_above(),
            };
            extension_row(
                p,
                format!("settings-skills-{}", sk.name),
                icon(&ic::POST_SKILL, 16.0).color(p.icon_muted),
                name_el,
                None,
                None,
                k + 1 == total,
            )
        })
        .collect();
    let mut out = Vec::new();
    if !rows.is_empty() {
        out.push(catalog_with(p, "settings-skills", rows, false));
    }
    if st.sources.failed() {
        out.push(load_failed::<H>(
            p,
            "settings-extensions-sources-failed",
            t!("settings.extensions.sources.loadFailed"),
            t!("common.retry"),
            retry_sources,
        ));
    }
    out
}
