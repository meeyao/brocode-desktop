//! A project's Extensions (`packages/app/src/settings/workspaces/project-extensions.tsx`):
//! MCPs, Plugins, Skills and LSPs under pill tabs. Each of the first three lists what the
//! project's folder adds to the server's own ("Added to this project", or empty copy), then
//! "Shared with all projects" folded under its count. Console-managed MCPs and virtual skills
//! link to Console and show "Replaces OpenCode Console" when overridden locally
//! (`anomalyco/opencode#63125`).

use opencode_i18n::t;
use opencode_kit::integration::{McpServer, McpStatus};
use opencode_kit::model::{ConsoleMcp, Project, SkillInfo};
use opencode_ui::focus::FocusExt;
use opencode_ui::palette::Pal;
use opencode_ui::widgets;
use rune::{El, Icon, TextStyle, Tx, col, icon, row, text};
use rune_theme_opencode::icons as ic;
use serde_json::Value;

use super::console_link::{ConsoleSources, console_connect_button, console_name, console_replaced, console_signed_in, load_failed};
use super::{Page, catalog, header, page};
use crate::host::SettingsHost;
use crate::jobs::{SettingsJob, SettingsRequest};

const TAB: TextStyle = TextStyle::new(13.0, 440.0).lh(16.0).tracking(-0.04);
const HEAD: TextStyle = TextStyle::new(13.0, 530.0).lh(16.0);
const BODY: TextStyle = TextStyle::new(13.0, 440.0).lh(20.0);
const NAME: TextStyle = TextStyle::new(13.0, 530.0).lh(16.0).tracking(-0.04);
const STATUS: TextStyle = TextStyle::new(11.0, 440.0).lh(16.0);
const COUNT: TextStyle = TextStyle::new(11.0, 530.0).lh(16.0);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Tab {
    #[default]
    Mcps,
    Plugins,
    Skills,
    Lsps,
}

/// A language server the configuration names (`configuredLanguageServers`).
#[derive(Clone, Debug)]
pub struct Lsp {
    pub name: String,
    pub disabled: bool,
    pub extensions: Vec<String>,
}

/// `(server_skills, project_skills)` loaded for a project's Extensions tab.
pub type SkillPairs = (Vec<SkillInfo>, Vec<SkillInfo>);

#[derive(Default)]
pub struct State {
    pub dir: String,
    pub tab: Tab,
    /// "Shared with all projects" unfolded.
    pub shared: bool,
    /// (the server's, the folder's), loading (`None`), or failed (`Some(Err(()))`).
    plugins: Option<Result<(Vec<String>, Vec<String>), ()>>,
    /// (the server's, the folder's)
    skills: Option<SkillPairs>,
    /// Console-managed MCP sources for this project folder.
    sources: ConsoleSources,
    /// The folder's configured servers, whether LSP is off, or a failed read.
    lsps: Option<Result<(bool, Vec<Lsp>), ()>>,
}

impl State {
    pub(crate) fn apply_loaded(
        &mut self,
        plugins: Result<(Vec<String>, Vec<String>), ()>,
        skills: Option<SkillPairs>,
        sources: Result<Vec<ConsoleMcp>, ()>,
        lsps: Result<(bool, Vec<Lsp>), ()>,
    ) {
        self.plugins = Some(plugins);
        // A failed read keeps the lists already read, as Desktop's location data does.
        self.skills = skills.or(self.skills.take());
        self.sources.apply(sources);
        self.lsps = Some(lsps);
    }
}

/// Reads the folder's and the server's extensions.
pub fn load(s: &mut dyn SettingsHost, dir: &str) {
    if s.settings_pages().project.extensions.dir != dir {
        s.settings_pages_mut().project.extensions = State { dir: dir.to_owned(), ..State::default() };
    }
    s.sync_mcp("");
    s.sync_mcp(dir);
    s.sync_catalogs(Some(dir));
    s.spawn_job(SettingsJob::ActiveServer(SettingsRequest::LoadProjectExtensions { dir: dir.to_owned() }));
}

/// Refreshes project extensions when `plugin.updated` or `config.updated` arrives for this folder.
pub fn on_updated(s: &mut dyn SettingsHost, event_dir: Option<&str>) {
    if !s.settings_open() {
        return;
    }
    let Page::Project(dir, super::project::Tab::Extensions) = &s.settings_pages().page else {
        return;
    };
    if let Some(loc) = event_dir
        && loc != dir
    {
        return;
    }
    let dir = dir.clone();
    s.spawn_job(SettingsJob::ActiveServer(SettingsRequest::LoadProjectExtensions { dir }));
}

/// The language servers a configuration's documents name, a later document's entry over an
/// earlier one's and `lsp: false` (or `true`) replacing them all.
pub fn configured_lsps_entries(entries: &[Value]) -> (bool, Vec<Lsp>) {
    let mut disabled = false;
    let mut servers: Vec<Lsp> = Vec::new();
    for e in entries.iter().filter(|e| e["type"] == "document") {
        let lsp = &e["info"]["lsp"];
        if let Some(on) = lsp.as_bool() {
            disabled = !on;
            servers.clear();
            continue;
        }
        let Some(map) = lsp.as_object() else { continue };
        disabled = false;
        for (name, server) in map {
            let before = servers.iter().position(|x| &x.name == name).map(|i| servers.remove(i));
            servers.push(Lsp {
                name: name.clone(),
                disabled: server["disabled"].as_bool().or(before.as_ref().map(|b| b.disabled)).unwrap_or(false),
                extensions: server["extensions"]
                    .as_array()
                    .map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_owned)).collect())
                    .or(before.map(|b| b.extensions))
                    .unwrap_or_default(),
            });
        }
    }
    servers.sort_by(|a, b| rune::text::compare_names(&a.name, &b.name));
    (disabled, servers)
}

fn select_tab(s: &mut dyn SettingsHost, tab: Tab, tx: &mut Tx) {
    tx.instant();
    s.settings_pages_mut().project.extensions.tab = tab;
}

fn toggle_shared(s: &mut dyn SettingsHost, tx: &mut Tx) {
    tx.instant();
    let st = &mut s.settings_pages_mut().project.extensions;
    st.shared = !st.shared;
}

fn retry_project(s: &mut dyn SettingsHost, _tx: &mut Tx) {
    let dir = s.settings_pages().project.extensions.dir.clone();
    s.settings_pages_mut().project.extensions.dir.clear();
    load(s, &dir);
}

pub fn view<H: SettingsHost + 'static>(s: &H, p: &Pal, x: &Project) -> El {
    let st = &s.settings_pages().project.extensions;
    let signed_in = console_signed_in(s, Some(&x.directory));
    let body = match st.tab {
        Tab::Mcps => {
            let shared: Vec<String> = names(s.mcp_servers("").unwrap_or_default().iter().map(|m| m.name.clone()).collect());
            let own: Vec<String> = names(
                s.mcp_servers(&x.directory)
                    .unwrap_or_default()
                    .iter()
                    .map(|m| m.name.clone())
                    .filter(|n| !shared.contains(n))
                    .collect(),
            );
            let make_rows = |list: &[String], key: &str| {
                let rows = list.iter().map(|name| mcp_row::<H>(s, p, &x.directory, &st.sources, key, name)).collect();
                catalog(p, key, rows)
            };
            section_with::<H>(
                s,
                p,
                "mcps",
                manage(p),
                own.is_empty(),
                None,
                (!own.is_empty()).then(|| make_rows(&own, "settings-project-mcps")),
                shared.len(),
                (!shared.is_empty()).then(|| make_rows(&shared, "settings-project-shared-mcps")),
                st.sources.failed(),
            )
        }
        Tab::Plugins => {
            let loaded = st.plugins.as_ref().and_then(|r| r.as_ref().ok());
            let (shared, own_raw) = loaded.cloned().unwrap_or_default();
            let own: Vec<String> = own_raw.into_iter().filter(|n| !shared.contains(n)).collect();
            let make_rows = |list: &[String], key: &str| {
                catalog(p, key, list.iter().map(|n| plain_row(p, &format!("{key}-{n}"), &ic::CUBE, n)).collect())
            };
            let status = match &st.plugins {
                None => Some(text(t!("common.loading")).font(BODY).color(p.muted)),
                Some(Err(())) => Some(load_failed::<H>(
                    p,
                    "project-plugins-failed",
                    t!("settings.extensions.plugins.loadFailed"),
                    t!("common.retry"),
                    retry_project,
                )),
                Some(Ok(_)) => None,
            };
            section_with::<H>(
                s,
                p,
                "plugins",
                manage(p),
                loaded.is_some() && own.is_empty(),
                status,
                (!own.is_empty()).then(|| make_rows(&own, "settings-project-plugins")),
                shared.len(),
                (!shared.is_empty()).then(|| make_rows(&shared, "settings-project-shared-plugins")),
                false,
            )
        }
        Tab::Skills => {
            let (shared, own_raw) = st.skills.clone().unwrap_or_default();
            let own: Vec<SkillInfo> = own_raw
                .into_iter()
                .filter(|sk| !shared.iter().any(|sh| sh.name == sk.name && sh.path == sk.path))
                .collect();
            let make_rows = |list: &[SkillInfo], key: &str| {
                let rows = list
                    .iter()
                    .map(|sk| skill_row::<H>(p, &st.sources, signed_in, &format!("{key}-{}", sk.name), sk))
                    .collect();
                catalog(p, key, rows)
            };
            section_with::<H>(
                s,
                p,
                "skills",
                skills_link::<H>(p),
                own.is_empty(),
                None,
                (!own.is_empty()).then(|| make_rows(&own, "settings-project-skills")),
                shared.len(),
                (!shared.is_empty()).then(|| make_rows(&shared, "settings-project-shared-skills")),
                st.sources.failed(),
            )
        }
        Tab::Lsps => lsps::<H>(p, st.lsps.as_ref()),
    };
    let content = col()
        .key("settings-project-extensions")
        .w_full()
        .gap(16.0)
        .children(s.settings_pages().project.missing.then(|| super::project::folder_missing(p, &x.directory)))
        .child(col().w_full().child(tabs::<H>(p, st.tab)).child(col().w_full().pt(24.0).child(body)));
    page(
        p,
        "settings-project-extensions-page",
        header(p, t!("settings.tab.extensions"), t!("project.settings.extensions.description")),
        content,
    )
}

fn mcp_row<H: SettingsHost + 'static>(
    s: &H,
    p: &Pal,
    dir: &str,
    sources: &ConsoleSources,
    prefix: &str,
    name: &str,
) -> El {
    let server = s
        .mcp_servers(dir)
        .and_then(|l| l.iter().find(|m| m.name == name))
        .or_else(|| s.mcp_servers("").and_then(|l| l.iter().find(|m| m.name == name)))
        .cloned()
        .unwrap_or(McpServer {
            name: name.to_owned(),
            status: McpStatus::Pending,
            integration: None,
        });
    let disabled = server.status == McpStatus::Disabled;
    let managed = sources.managed(name);
    let links = sources.links(name);
    let dot_status = if managed.is_some_and(|c| c.needs_connection) && server.status == McpStatus::Connected {
        "needs_auth"
    } else {
        server.status.name()
    };
    let name_el = match (managed, &links) {
        (Some(_), Some(l)) => console_name::<H>(p, format!("{prefix}-console-{name}"), name, l.manage.clone(), disabled),
        _ => text(name.to_owned()).font(NAME).color(if disabled { p.muted } else { p.text }).truncate().help_when_truncated(name.to_owned()).help_above().shrink().min_w(0.0),
    };
    let badge = sources
        .mcp(name)
        .filter(|c| c.overridden)
        .and(links.as_ref())
        .map(|l| console_replaced::<H>(p, format!("{prefix}-replaced-{name}"), name, l.manage.clone()));
    let trailing = match sources.connect(name) {
        Some(connect_url) => console_connect_button::<H>(p, format!("{prefix}-connect-{name}"), name, connect_url),
        None => s.mcp_row_trailing(p, dir, &server),
    };
    row()
        .key(format!("{prefix}-{name}"))
        .w_full()
        .min_h(56.0)
        .py(16.0)
        .gap(16.0)
        .center_y()
        .child(
            row()
                .grow(1.0)
                .shrink()
                .min_w(0.0)
                .gap(8.0)
                .center_y()
                .child(widgets::status_dot(p, dot_status))
                .child(name_el)
                .children(badge),
        )
        .child(trailing)
}

fn skill_row<H: SettingsHost + 'static>(
    p: &Pal,
    sources: &ConsoleSources,
    signed_in: bool,
    key: &str,
    skill: &SkillInfo,
) -> El {
    let name_el = match sources.skill(skill, signed_in) {
        Some(href) => console_name::<H>(p, format!("{key}-console"), &skill.name, href, false),
        None => text(skill.name.clone()).font(NAME).color(p.text).truncate().help_when_truncated(skill.name.clone()).help_above().shrink().min_w(0.0),
    };
    row()
        .key(key.to_owned())
        .w_full()
        .min_h(56.0)
        .py(16.0)
        .gap(8.0)
        .center_y()
        .child(icon(&ic::POST_SKILL, 16.0).color(p.icon_muted))
        .child(name_el)
}

fn names(mut v: Vec<String>) -> Vec<String> {
    v.sort();
    v.dedup();
    v
}

/// Pill tabs, as Extensions' (`tabs-v2` pill).
fn tabs<H: SettingsHost + 'static>(p: &Pal, on: Tab) -> El {
    let mut list = row().key("settings-project-extensions-tabs").role("tablist").focus_group().gap(6.0).pb(8.0);
    for (tab, id, label) in [
        (Tab::Mcps, "mcps", t!("settings.extensions.tab.mcps")),
        (Tab::Plugins, "plugins", t!("settings.extensions.tab.plugins")),
        (Tab::Skills, "skills", t!("settings.extensions.tab.skills")),
        (Tab::Lsps, "lsps", t!("project.settings.extensions.tab.lsps")),
    ] {
        let picked = tab == on;
        let pill = row()
            .key(format!("settings-project-extensions-tab-{id}"))
            .role("tab")
            .label(label)
            .focus_selected(picked)
            .focus_default(p)
            .h(24.0)
            .px(8.0)
            .center()
            .radius(6.0)
            .on_press_tx(move |s: &mut H, tx: &mut Tx| select_tab(s, tab, tx))
            .child(text(label).font(TAB).color(if picked { p.text } else { p.muted }));
        list = list.child(if picked { pill.fill(p.layer3).box_shadow(Pal::ring_in(p.border_muted)) } else { pill.hover(p.hover) });
    }
    list
}

#[allow(clippy::too_many_arguments)]
fn section_with<H: SettingsHost + 'static>(
    s: &H,
    p: &Pal,
    kind: &str,
    action: El,
    empty: bool,
    status: Option<El>,
    own_card: Option<El>,
    shared_count: usize,
    shared_card: Option<El>,
    sources_failed: bool,
) -> El {
    let (none, none_about) = match kind {
        "mcps" => (t!("project.settings.extensions.empty.mcps.title"), t!("project.settings.extensions.empty.mcps.description")),
        "plugins" => (t!("project.settings.extensions.empty.plugins.title"), t!("project.settings.extensions.empty.plugins.description")),
        _ => (t!("project.settings.extensions.empty.skills.title"), t!("project.settings.extensions.empty.skills.description")),
    };
    let title = if empty { none } else { t!("project.settings.extensions.added") };
    let copy = col()
        .grow(1.0)
        .shrink()
        .min_w(0.0)
        .gap(6.0)
        .child(text(title).font(HEAD).color(p.text).wrap())
        .children(empty.then(|| text(none_about).font(BODY).color(p.muted).wrap()));
    let head = row().key("project-settings-extension-section-header").w_full().gap(16.0).items_start().child(copy).child(action);
    let mut c = col().key(format!("project-settings-extension-{kind}")).w_full().gap(16.0).child(head).children(status);
    if let Some(card) = own_card {
        c = c.child(card);
    }
    if shared_count > 0 {
        let open = s.settings_pages().project.extensions.shared;
        let trigger = row()
            .key("project-settings-shared-trigger")
            .role("button")
            .label_static(t!("project.settings.extensions.shared"))
            .expanded(open)
            .h(28.0)
            .pl(4.0)
            .pr(8.0)
            .gap(4.0)
            .center_y()
            .radius(4.0)
            .self_start()
            .focus_default(p)
            .hover(p.layer2)
            .on_press_tx(|s: &mut H, tx: &mut Tx| toggle_shared(s, tx))
            .child(
                rune::el()
                    .size(20.0, 20.0)
                    .center()
                    .child(icon(&ic::FILL_TRIANGLE_DOWN, 16.0).color(p.icon_muted).rotate(if open { 0.0 } else { -std::f32::consts::FRAC_PI_2 }).mirror_in_rtl()),
            )
            .child(
                row()
                    .gap(6.0)
                    .center_y()
                    .child(text(t!("project.settings.extensions.shared")).font(HEAD).color(p.text))
                    .child(row().h(16.0).min_w(16.0).px(4.0).center().radius(3.0).fill(p.layer2).child(text(shared_count.to_string()).font(COUNT).color(p.muted))),
            );
        c = c.child(col().w_full().gap(12.0).mt(16.0).child(trigger).children(open.then_some(shared_card).flatten()));
    }
    if sources_failed {
        c = c.child(load_failed::<H>(
            p,
            "project-settings-sources-failed",
            t!("settings.extensions.sources.loadFailed"),
            t!("common.retry"),
            retry_project,
        ));
    }
    c
}

fn plain_row(p: &Pal, key: &str, glyph: &'static Icon, name: &str) -> El {
    row()
        .key(key.to_owned())
        .w_full()
        .min_h(56.0)
        .py(16.0)
        .gap(8.0)
        .center_y()
        .child(icon(glyph, 16.0).color(p.icon_muted))
        .child(text(name.to_owned()).font(NAME).color(p.text).truncate().help_when_truncated(name.to_owned()).help_above().shrink().min_w(0.0))
}

fn manage(p: &Pal) -> El {
    text(t!("settings.extensions.manageConfig")).font(BODY).color(p.muted)
}

fn skills_link<H: SettingsHost + 'static>(p: &Pal) -> El {
    let words = t!("settings.extensions.addSkills");
    const URL: &str = "https://github.com/meeyao/brocode-desktop";
    text(words)
        .key("settings-extension-link")
        .font(BODY)
        .color(p.accent)
        .spans(vec![rune::Span::new(0, words.len()).underline(p.accent)])
        .link(rune::Link::new(0..words.len(), URL).role("link").label(words).hover(rune::LinkPaint::new().color(p.accent_hover).underline(p.accent_hover)))
        .on_link(|s: &mut H, url: &str, tx: &mut Tx| crate::open_external(s, url, tx))
}

fn lsps<H: SettingsHost + 'static>(p: &Pal, read: Option<&Result<(bool, Vec<Lsp>), ()>>) -> El {
    let (disabled, servers) = match read {
        Some(Ok((d, list))) => (*d, list.as_slice()),
        _ => (false, &[][..]),
    };
    let empty = servers.is_empty();
    let (title, about) = match (empty, disabled) {
        (true, true) => (t!("project.settings.extensions.lsp.disabled.title"), Some(t!("project.settings.extensions.lsp.disabled.description"))),
        (true, false) => (t!("project.settings.extensions.lsp.empty.title"), Some(t!("project.settings.extensions.lsp.empty.description"))),
        _ => (t!("project.settings.extensions.lsp.configured"), None),
    };
    let copy = col()
        .grow(1.0)
        .shrink()
        .min_w(0.0)
        .gap(6.0)
        .child(text(title).font(HEAD).color(p.text).wrap())
        .children(about.map(|a| text(a).font(BODY).color(p.muted).wrap()));
    let head = row().w_full().gap(16.0).items_start().child(copy).child(text(t!("project.settings.extensions.lsp.description")).font(BODY).color(p.muted));
    let mut c = col().key("project-settings-extension-lsps").w_full().gap(16.0).child(head);
    match read {
        None => c = c.child(text(t!("common.loading")).font(BODY).color(p.muted)),
        Some(Err(())) => {
            c = c.child(load_failed::<H>(
                p,
                "project-lsp-failed",
                t!("project.settings.extensions.lsp.loadFailed"),
                t!("project.settings.extensions.lsp.retry"),
                retry_project,
            ));
        }
        Some(Ok(_)) if !empty => {
            let rows = servers
                .iter()
                .map(|l| {
                    let copy = col()
                        .grow(1.0)
                        .shrink()
                        .min_w(0.0)
                        .gap(4.0)
                        .child(text(l.name.clone()).font(NAME).color(p.text).truncate().help_when_truncated(l.name.clone()).help_above())
                        .children((!l.extensions.is_empty()).then(|| text(l.extensions.join(", ")).font(BODY).color(p.muted).wrap()));
                    row()
                        .key(format!("settings-project-lsp-{}", l.name))
                        .w_full()
                        .min_h(56.0)
                        .py(16.0)
                        .gap(16.0)
                        .center_y()
                        .child(row().grow(1.0).shrink().min_w(0.0).gap(8.0).center_y().child(icon(&ic::CODE, 16.0).color(p.icon_muted)).child(copy))
                        .child(
                            row()
                                .gap(6.0)
                                .center_y()
                                .child(text(if l.disabled { t!("project.settings.extensions.lsp.status.disabled") } else { t!("project.settings.extensions.lsp.status.enabled") }).font(STATUS).color(p.muted)),
                        )
                })
                .collect();
            c = c.child(catalog(p, "settings-project-lsps", rows));
        }
        _ => {}
    }
    c
}
