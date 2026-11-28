//! Native/drawn page right-click context menus (`packages/gui-plugins/src/browser/{page-menu,
//! native-page-menu}.ts`) and privacy-safe structured page diagnostics (`UP-110` `#65096`,
//! `page-log.ts`).

use std::fs::OpenOptions;
use std::io::Write as _;

use browser_pane::{
    ContextMenuCommand, DownloadSaveRequest, ExtensionMenuItem, ExtensionMenuItemKind,
    PageContextMenu, PageDiagnostic,
};
use opencode_i18n::t;
use rune::{FileDialogOptions, Tx};

use crate::App;
use crate::popup::{Item, Menu};

const MAX_DIAG_LOG_BYTES: u64 = 1024 * 1024;

/// Polls engine-raised page context menus and structured page diagnostics (zero work when empty).
pub fn poll(app: &mut App, tx: &mut Tx) -> bool {
    let mut changed = false;
    if app.workspace_panes.browser.host.has_context_menus() {
        let pending: Vec<(u64, PageContextMenu)> =
            app.workspace_panes.browser.host.drain_context_menus().collect();
        for (tab_id, menu) in pending {
            open_page_menu(app, tab_id, menu, tx);
            changed = true;
        }
    }
    if app.workspace_panes.browser.host.has_diagnostics() {
        let records: Vec<PageDiagnostic> =
            app.workspace_panes.browser.host.drain_diagnostics().collect();
        log_diagnostics(app, records);
    }
    if !app.workspace_panes.browser.save_dialog_open
        && let Some(req) = app.workspace_panes.browser.host.pop_download_save_request()
    {
        open_download_save_dialog(app, req, tx);
        changed = true;
    }
    changed
}

fn open_download_save_dialog(app: &mut App, req: DownloadSaveRequest, tx: &mut Tx) {
    let Some(tab) = app.workspace_panes.browser.host.tab(req.tab_id) else {
        app.workspace_panes.browser.host.answer_download_save(req.tab_id, req.id, None);
        return;
    };
    if tab.preview.is_some() || tab.refusal.as_deref() == Some("browser.proxy.required") {
        app.workspace_panes.browser.host.answer_download_save(req.tab_id, req.id, None);
        return;
    }
    app.workspace_panes.browser.save_dialog_open = true;
    let options = FileDialogOptions {
        title: t!("desktop.dialog.saveFile").to_owned(),
        file_name: Some(browser_pane::policy::sanitize_download_name(&req.name)),
        ..Default::default()
    };
    let tab_id = req.tab_id;
    let id = req.id;
    tx.save_file(options, move |s: &mut App, answer, _| {
        s.workspace_panes.browser.save_dialog_open = false;
        let chosen = match answer {
            Ok(Some(path))
                if s.workspace_panes
                    .browser
                    .host
                    .tab(tab_id)
                    .is_some_and(|t| {
                        t.preview.is_none()
                            && t.refusal.as_deref() != Some("browser.proxy.required")
                    }) =>
            {
                path.to_str().map(str::to_owned)
            }
            _ => None,
        };
        s.workspace_panes.browser.host.answer_download_save(tab_id, id, chosen);
    });
}

fn live_driver(
    captured: Option<browser_pane::TabDriver>,
    live_tab: &browser_pane::BrowserTab,
) -> Option<browser_pane::TabDriver> {
    if live_tab.prompts.agent_driven {
        let live = live_tab.prompts.driver.clone().unwrap_or(browser_pane::TabDriver::Agent);
        if live == browser_pane::TabDriver::Agent || captured.is_none() {
            return Some(live);
        }
    }
    captured
}

fn open_page_menu(app: &mut App, tab_id: u64, info: PageContextMenu, tx: &mut Tx) {
    let Some(tab) = app.workspace_panes.browser.host.tab(tab_id) else {
        app.workspace_panes.browser.host.run_context_menu_action(tab_id, info.id, None);
        return;
    };
    if tab.refusal.as_deref() == Some("browser.proxy.required") {
        // Suspended remote tab: cancel immediately rather than showing non-functional commands.
        app.workspace_panes.browser.host.run_context_menu_action(tab_id, info.id, None);
        return;
    }
    let can_back = tab.can_go_back;
    let can_forward = tab.can_go_forward;
    let is_preview = tab.preview.is_some();
    let menu_extras = app.workspace_panes.browser.host.engine_menu_extras();
    let host_clipboard = app.workspace_panes.browser.host.uses_host_clipboard();
    let (ox, oy) = tab.surface_origin;
    let z = tab.view_zoom.max(0.01);
    let anchor = (ox + (info.x as f32) / z, oy + (info.y as f32) / z);
    let can_open_page = app.browser_tab_external_url(tab_id).is_some();
    let allow_ext_menu = tab.preview.is_none()
        && !tab.prompts.agent_driven
        && matches!(tab.context, browser_pane::ContextKind::Default);
    let captured_opener = app.browser_context_for_opener(tab_id);
    let captured_sid = app.browser_tab_session(tab_id).map(str::to_owned);
    let captured_external_link_ok = info
        .link_url
        .as_deref()
        .is_some_and(|u| app.external_url_allowed_for_tab(tab_id, u));

    let mut groups: Vec<Vec<Item>> = Vec::new();

    // 1. Navigation (`page-menu.ts:81-86`)
    groups.push(vec![
        Item::new("back", t!("rune.browser.menu.back")).enabled(can_back),
        Item::new("forward", t!("rune.browser.menu.forward")).enabled(can_forward),
        Item::new("reload", t!("rune.browser.menu.reload")),
        Item::new("hard-reload", t!("rune.browser.menu.hardReload")),
    ]);

    // 2. Open page in default browser (`page-menu.ts:153`)
    groups.push(vec![if can_open_page {
        Item::new("open-page", t!("rune.browser.menu.openPage"))
    } else {
        Item::new("open-page-unavailable", t!("rune.browser.menu.pageUnavailable")).enabled(false)
    }]);

    // 3. Link actions (`page-menu.ts:88-96`)
    if let Some(link) = &info.link_url {
        let mut items = vec![Item::new("open-link", t!("rune.browser.menu.openLink"))];
        if app.external_url_allowed_for_tab(tab_id, link) {
            items.push(Item::new("external-link", t!("rune.browser.menu.external")));
        }
        items.push(Item::new("copy-link", t!("rune.browser.menu.copyLink")));
        groups.push(items);
    }

    // 4. Image actions (`page-menu.ts:98-105`)
    if info.is_image && info.src_url.is_some() {
        let mut items = vec![
            Item::new("open-image", t!("rune.browser.menu.openImage")),
            Item::new("save-image", t!("rune.browser.menu.saveImage")).enabled(info.has_image_contents),
        ];
        if menu_extras && !is_preview {
            items.push(Item::new("copy-image", t!("rune.browser.menu.copyImage")).enabled(info.has_image_contents));
        }
        items.push(Item::new("copy-image-address", t!("rune.browser.menu.copyImageAddress")));
        groups.push(items);
    }

    // 5. Spelling suggestions (`page-menu.ts:119-128`)
    let show_spelling = !menu_extras || info.spellcheck;
    if info.is_editable && show_spelling && let Some(word) = &info.misspelled_word && !word.is_empty() {
        let mut items: Vec<Item> = info
            .suggestions
            .iter()
            .enumerate()
            .map(|(i, s)| Item::new(format!("replace:{i}"), s.clone()))
            .collect();
        items.push(Item::new("add-word", t!("rune.browser.menu.addWord")));
        groups.push(items);
    }

    // 6. Edit / selection (`page-menu.ts:107-117`)
    // When CEF runs on X11 Ozone inside a Wayland session, its `can_paste` checks X11 rather than
    // Rune's Wayland clipboard, so keep Paste enabled on editable fields and read the host clipboard.
    let can_paste = info.can_paste || host_clipboard;
    if info.is_editable {
        groups.push(vec![
            Item::new("cut", t!("rune.browser.menu.cut")).enabled(info.can_cut),
            Item::new("copy", t!("rune.browser.menu.copy")).enabled(info.can_copy),
            Item::new("paste", t!("rune.browser.menu.paste")).enabled(can_paste),
            Item::new("paste-plain", t!("rune.browser.menu.pastePlain")).enabled(can_paste),
            Item::new("select-all", t!("rune.browser.menu.selectAll")).enabled(info.can_select_all),
        ]);
    } else if !info.selection_text.is_empty() {
        groups.push(vec![Item::new("copy", t!("rune.browser.menu.copy")).enabled(info.can_copy)]);
    }

    let mut menu = Menu::new(t!("rune.browser.menu.page"));
    for (idx, items) in groups.into_iter().enumerate() {
        if idx > 0 {
            menu = menu.separator();
        }
        for item in items {
            menu = menu.item(item);
        }
    }

    // 7. Spellcheck toggle & languages (`page-menu.ts:130-150`, gated on `Ready.menu_extras`)
    if menu_extras && info.is_editable {
        menu = menu
            .separator()
            .item(Item::new("spellcheck", t!("rune.browser.menu.spellcheck")).checked(info.spellcheck));
        if !info.dictionaries.is_empty() {
            let mut langs = Menu::new(t!("rune.browser.menu.languages"));
            for tag in &info.dictionaries {
                let checked = info.languages.iter().any(|l| l == tag);
                let enabled = !(info.languages.len() == 1 && info.languages[0] == *tag);
                langs = langs.item(Item::new(format!("lang:{tag}"), tag.clone()).checked(checked).enabled(enabled));
            }
            menu = menu.submenu_enabled("languages", t!("rune.browser.menu.languages"), info.spellcheck, langs);
        }
    }

    // 8. Extension context-menu items (`page-menu.ts:155`, `context-menus.ts:235-268`)
    let mut ext_actions: Vec<(String, String)> = Vec::new();
    if allow_ext_menu && !info.extensions.is_empty() {
        menu = menu.separator();
        for group in &info.extensions {
            let sub = build_extension_submenu(&group.title, &group.ext_id, &group.items, &mut ext_actions, 0);
            if !sub.is_empty() {
                menu = menu.submenu(format!("ext-group:{}", group.ext_id), group.title.clone(), sub);
            }
        }
    }

    let menu_id = info.id;
    let link_url = info.link_url;
    let src_url = info.src_url;
    let misspelled = info.misspelled_word;
    let suggestions = info.suggestions;
    let dictionaries = info.dictionaries;

    tx.instant();
    crate::popup::open(app, menu, anchor, tx, move |s: &mut App, picked: Option<&str>, tx: &mut Tx| {
        let mut engine_cmd: Option<ContextMenuCommand> = None;
        match picked {
            Some("back") => engine_cmd = Some(ContextMenuCommand::Back),
            Some("forward") => engine_cmd = Some(ContextMenuCommand::Forward),
            Some("reload") => engine_cmd = Some(ContextMenuCommand::Reload),
            Some("hard-reload") => engine_cmd = Some(ContextMenuCommand::HardReload),
            Some("open-page") => s.open_browser_tab_externally(tab_id, tx),
            Some("open-link") => {
                if let Some(url) = link_url
                    && let Some(live_tab) = s.workspace_panes.browser.host.tab(tab_id)
                    && let Some((captured_driver, mut ctx)) = captured_opener.clone()
                {
                    let driver = live_driver(captured_driver, live_tab);
                    let proxy_ok = match &mut ctx {
                        browser_pane::ContextKind::RemoteServer { server_id, proxy } => {
                            *proxy = s.workspace_panes.browser.host.remote_proxy(server_id.trim()).cloned();
                            proxy.is_some()
                        }
                        _ => true,
                    };
                    if proxy_ok {
                        let sid = s.browser_tab_session(tab_id).map(str::to_owned).or_else(|| captured_sid.clone());
                        s.open_new_browser_tab_beside(sid.as_deref(), &url, tab_id, true, ctx, driver);
                    }
                }
            }
            Some("external-link") => {
                if let Some(url) = link_url
                    && captured_external_link_ok
                    && s.external_url_allowed_for_tab(tab_id, &url)
                {
                    crate::open_in::open_external(tx, &url);
                }
            }
            Some("copy-link") => {
                if let Some(url) = link_url {
                    tx.copy(url);
                }
            }
            Some("open-image") => {
                if let Some(url) = src_url
                    && let Some(live_tab) = s.workspace_panes.browser.host.tab(tab_id)
                    && let Some((captured_driver, mut ctx)) = captured_opener.clone()
                {
                    let driver = live_driver(captured_driver, live_tab);
                    let proxy_ok = match &mut ctx {
                        browser_pane::ContextKind::RemoteServer { server_id, proxy } => {
                            *proxy = s.workspace_panes.browser.host.remote_proxy(server_id.trim()).cloned();
                            proxy.is_some()
                        }
                        _ => true,
                    };
                    if proxy_ok {
                        let sid = s.browser_tab_session(tab_id).map(str::to_owned).or_else(|| captured_sid.clone());
                        s.open_new_browser_tab_beside(sid.as_deref(), &url, tab_id, true, ctx, driver);
                    }
                }
            }
            Some("save-image") => {
                if let Some(url) = src_url {
                    engine_cmd = Some(ContextMenuCommand::SaveImage { url });
                }
            }
            Some("copy-image") => {
                if menu_extras && !is_preview {
                    engine_cmd = Some(ContextMenuCommand::CopyImage);
                }
            }
            Some("copy-image-address") => {
                if let Some(url) = src_url {
                    tx.copy(url);
                }
            }
            Some("cut") => {
                engine_cmd = Some(if s.workspace_panes.browser.host.uses_host_clipboard() {
                    ContextMenuCommand::CutToHost
                } else {
                    ContextMenuCommand::Cut
                });
            }
            Some("copy") => {
                engine_cmd = Some(if s.workspace_panes.browser.host.uses_host_clipboard() {
                    ContextMenuCommand::CopyToHost
                } else {
                    ContextMenuCommand::Copy
                });
            }
            Some("paste") => {
                if s.workspace_panes.browser.host.uses_host_clipboard() {
                    tx.read_clipboard(move |s: &mut App, content, _| {
                        s.workspace_panes.browser.host.paste_host_clipboard(tab_id, content.text());
                    });
                } else {
                    engine_cmd = Some(ContextMenuCommand::Paste);
                }
            }
            Some("paste-plain") => {
                if s.workspace_panes.browser.host.uses_host_clipboard() {
                    tx.read_clipboard(move |s: &mut App, content, _| {
                        s.workspace_panes.browser.host.paste_host_clipboard(tab_id, content.text());
                    });
                } else {
                    engine_cmd = Some(ContextMenuCommand::PastePlain);
                }
            }
            Some("select-all") => engine_cmd = Some(ContextMenuCommand::SelectAll),
            Some("spellcheck") => {
                if menu_extras {
                    engine_cmd = Some(ContextMenuCommand::ToggleSpellcheck);
                }
            }
            Some("add-word") => {
                if let Some(word) = misspelled {
                    engine_cmd = Some(ContextMenuCommand::AddToDictionary { word });
                }
            }
            Some(other) => {
                if let Some(rest) = other.strip_prefix("replace:")
                    && let Ok(idx) = rest.parse::<usize>()
                    && let Some(word) = suggestions.get(idx).cloned()
                {
                    engine_cmd = Some(ContextMenuCommand::ReplaceMisspelling { word });
                } else if menu_extras
                    && let Some(tag) = other.strip_prefix("lang:")
                    && dictionaries.iter().any(|d| d == tag)
                {
                    engine_cmd = Some(ContextMenuCommand::ToggleLanguage { value: tag.to_owned() });
                } else if let Some(rest) = other.strip_prefix("ext:")
                    && let Ok(idx) = rest.parse::<usize>()
                    && let Some((ext_id, item_id)) = ext_actions.get(idx).cloned()
                {
                    engine_cmd = Some(ContextMenuCommand::ExtensionItem { ext_id, item_id });
                }
            }
            None => {}
        }
        s.workspace_panes
            .browser
            .host
            .run_context_menu_action(tab_id, menu_id, engine_cmd);
    });
}

fn build_extension_submenu(
    label: &str,
    ext_id: &str,
    items: &[ExtensionMenuItem],
    actions: &mut Vec<(String, String)>,
    depth: usize,
) -> Menu {
    let mut menu = Menu::new(label);
    if depth >= 6 {
        return menu;
    }
    for item in items {
        if item.kind == ExtensionMenuItemKind::Separator {
            menu = menu.separator();
            continue;
        }
        if !item.children.is_empty() {
            let child = build_extension_submenu(&item.title, ext_id, &item.children, actions, depth + 1);
            if !child.is_empty() {
                menu = menu.submenu(format!("ext-sub:{}:{}", ext_id, actions.len()), item.title.clone(), child);
            }
            continue;
        }
        let idx = actions.len();
        actions.push((ext_id.to_owned(), item.id.clone()));
        let mut entry = Item::new(format!("ext:{idx}"), item.title.clone()).enabled(item.enabled);
        match item.kind {
            ExtensionMenuItemKind::Checkbox => entry = entry.checked(item.checked),
            ExtensionMenuItemKind::Radio => entry = entry.radio(item.checked),
            ExtensionMenuItemKind::Normal | ExtensionMenuItemKind::Separator => {}
        }
        menu = menu.item(entry);
    }
    menu
}

fn log_diagnostics(app: &App, records: Vec<PageDiagnostic>) {
    let mut lines = Vec::with_capacity(records.len());
    for rec in records {
        let line = serde_json::json!({
            "level": rec.level,
            "event": rec.event,
            "tab": rec.tab_id,
            "data": rec.fields,
        })
        .to_string();
        eprintln!("browser-page: {line}");
        lines.push(line);
    }
    if !app.persists() || lines.is_empty() {
        return;
    }
    let Some(logs_dir) = crate::dirs::logs() else { return };
    // Append off the UI thread so Export Logs captures crash diagnostics (`ours.md` §1.2).
    let _ = std::thread::Builder::new().name("browser-page-log".into()).spawn(move || {
        if std::fs::create_dir_all(&logs_dir).is_err() {
            return;
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            let _ = std::fs::set_permissions(&logs_dir, std::fs::Permissions::from_mode(0o700));
        }
        let path = logs_dir.join("browser-page.log");
        if std::fs::metadata(&path).is_ok_and(|m| m.len() >= MAX_DIAG_LOG_BYTES) {
            let _ = std::fs::remove_file(&path);
        }
        let mut opts = OpenOptions::new();
        opts.create(true).append(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt as _;
            opts.mode(0o600);
        }
        if let Ok(mut file) = opts.open(&path) {
            for line in lines {
                let _ = writeln!(file, "{line}");
            }
        }
    });
}

impl App {
    /// Routes host-bridged browser text copies/cuts (`EngineMsg::CopiedText`) and Copy image
    /// results (`EngineMsg::CopiedImage`) to Rune's host clipboard.
    pub(super) fn copy_browser_clipboard(&mut self) {
        let texts: Vec<String> = self.workspace_panes.browser.host.drain_copied_texts().collect();
        for text in texts {
            self.later(Box::new(move |_s: &mut App, tx: &mut Tx| {
                tx.copy(text);
            }));
        }
        let images: Vec<String> = self.workspace_panes.browser.host.drain_copied_images().collect();
        for png_b64 in images {
            let Ok(png) = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, png_b64) else { continue };
            if let Some(image) = rune::ClipboardImage::from_png(png) {
                self.later(Box::new(move |_s: &mut App, tx: &mut Tx| {
                    tx.copy_rich(rune::ClipboardContent::default().with_image(image));
                }));
            }
        }
    }
}