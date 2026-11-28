//! CEF `ContextMenuHandler` replacing Chromium's default popup menu with a policy-checked
//! descriptor sent to the host (`packages/gui-plugins/src/browser/{page-menu,native-page-menu}.ts`).
//!
//! Security invariants:
//! - Chromium's built-in `MenuModel` is cleared (`model.clear()`) and `run_context_menu` always
//!   returns `1`, so unmanaged DevTools / View Source / Print / Save-as entries never appear.
//! - Link and image URLs are checked against `browser_policy` before reaching the menu or
//!   executing `SaveImage`.

use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};

use cef::rc::Rc as _;
use cef::{
    Browser, BrowserHost, CefString, CefStringList, ContextMenuHandler, ContextMenuParams, Frame,
    ImplBrowser, ImplBrowserHost, ImplContextMenuHandler, ImplContextMenuParams, ImplFrame,
    ImplMenuModel, ImplRunContextMenuCallback, MenuModel, RunContextMenuCallback,
    WrapContextMenuHandler, sys, wrap_context_menu_handler,
};

use crate::output::Output;
use crate::prompts::PromptShared;
use crate::wire::{ContextMenuCommand, EngineMsg, PageContextMenu};

/// Reads strings out of a borrowed CEF string list without taking ownership.
pub fn read_cef_string_list(list: &CefStringList) -> Vec<String> {
    let raw: *const sys::_cef_string_list_t = list.into();
    if raw.is_null() {
        return Vec::new();
    }
    // SAFETY: `list` is a live CEF string list borrowed for the duration of the callback.
    unsafe {
        let count = sys::cef_string_list_size(raw as *mut _);
        let mut out = Vec::with_capacity(count);
        for i in 0..count {
            let mut val: sys::cef_string_t = std::mem::zeroed();
            if sys::cef_string_list_value(raw as *mut _, i, &mut val) > 0 {
                let s = CefString::from(std::ptr::from_ref(&val)).to_string();
                sys::cef_string_utf16_clear(&mut val);
                if !s.is_empty() {
                    out.push(s);
                }
            }
        }
        out
    }
}

struct HeldMenu {
    id: u64,
    src_url: Option<String>,
    target: crate::extensions::context_menus::MenuTarget,
    /// The menu was for an editable field: only then do Spellcheck / Languages run.
    editable: bool,
    /// Full (wire-bounded) selection captured at menu open for host-clipboard Copy/Cut.
    selection_text: String,
    can_copy: bool,
    can_cut: bool,
    /// The frame the menu opened in (CEF's identifier, which is the DevTools frame id).
    frame_id: Option<String>,
    callback: RunContextMenuCallback,
}

/// What the engine still does after a menu command ran.
pub enum MenuFollowUp {
    /// An extension's item: the engine tells the extension (`contextMenus.onClicked`).
    Extension { ext_id: String, item_id: String, target: crate::extensions::context_menus::MenuTarget },
    /// Copy image: the engine reads and re-encodes the image (`menu_extras.rs`).
    CopyImage { url: String, frame_id: Option<String> },
    /// Host-bridged Copy/Cut: text captured directly from `ContextMenuParams::selection_text`.
    CopyText { text: String },
    /// Host-bridged Copy/Cut fallback via isolated-world selection reader.
    CopySelection { cut: bool },
}

/// Per-tab held CEF context menu callback (at most one active at a time).
#[derive(Default)]
pub struct TabMenuState {
    held: Mutex<Option<HeldMenu>>,
}

impl TabMenuState {
    pub fn cancel_all(&self) {
        if let Some(prev) = self.held.lock().unwrap().take() {
            prev.callback.cancel();
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn execute(
        &self,
        tab_id: u64,
        tab_gen: u64,
        id: u64,
        action: Option<ContextMenuCommand>,
        browser: &Browser,
        host: &BrowserHost,
        _file_roots: &[String],
        block_downloads: bool,
        downloads: &crate::downloads::Downloads,
    ) -> Option<MenuFollowUp> {
        let matched = {
            let mut slot = self.held.lock().unwrap();
            if slot.as_ref().is_some_and(|h| h.id == id) {
                slot.take()
            } else {
                None
            }
        };
        // Always cancel CEF's native menu runner; we execute the chosen action directly.
        let held = matched?;
        held.callback.cancel();
        let held_src_url = held.src_url;
        let cmd = action?;
        match cmd {
            // `src_url` is held only for an image with contents, outside preview tabs.
            ContextMenuCommand::CopyImage => {
                return held_src_url.map(|url| MenuFollowUp::CopyImage { url, frame_id: held.frame_id });
            }
            ContextMenuCommand::ToggleSpellcheck => {
                if held.editable
                    && let Some(ctx) = host.request_context()
                {
                    crate::menu_extras::toggle_spellcheck(&ctx);
                }
            }
            ContextMenuCommand::ToggleLanguage { value } => {
                if held.editable
                    && let Some(ctx) = host.request_context()
                {
                    crate::menu_extras::toggle_language(&ctx, &value);
                }
            }
            ContextMenuCommand::Back => browser.go_back(),
            ContextMenuCommand::Forward => browser.go_forward(),
            ContextMenuCommand::Reload => browser.reload(),
            ContextMenuCommand::HardReload => browser.reload_ignore_cache(),
            ContextMenuCommand::Cut => {
                if let Some(f) = browser.focused_frame().or_else(|| browser.main_frame()) {
                    f.cut();
                }
            }
            ContextMenuCommand::Copy => {
                if let Some(f) = browser.focused_frame().or_else(|| browser.main_frame()) {
                    f.copy();
                }
            }
            ContextMenuCommand::CutToHost => {
                if held.can_cut {
                    if !held.selection_text.is_empty() {
                        if let Some(f) = browser.focused_frame().or_else(|| browser.main_frame()) {
                            f.del();
                        }
                        return Some(MenuFollowUp::CopyText { text: held.selection_text });
                    }
                    return Some(MenuFollowUp::CopySelection { cut: true });
                }
            }
            ContextMenuCommand::CopyToHost => {
                if held.can_copy {
                    if !held.selection_text.is_empty() {
                        return Some(MenuFollowUp::CopyText { text: held.selection_text });
                    }
                    return Some(MenuFollowUp::CopySelection { cut: false });
                }
            }
            ContextMenuCommand::Paste => {
                if let Some(f) = browser.focused_frame().or_else(|| browser.main_frame()) {
                    f.paste();
                }
            }
            ContextMenuCommand::PastePlain => {
                if let Some(f) = browser.focused_frame().or_else(|| browser.main_frame()) {
                    f.paste_and_match_style();
                }
            }
            ContextMenuCommand::SelectAll => {
                if let Some(f) = browser.focused_frame().or_else(|| browser.main_frame()) {
                    f.select_all();
                }
            }
            ContextMenuCommand::SaveImage { url } => {
                if !block_downloads
                    && held_src_url.as_deref() == Some(url.as_str())
                    && browser_policy::download_url_allowed(&url)
                {
                    downloads.mark_pending_save_image(tab_id, tab_gen, &url);
                    host.start_download(Some(&CefString::from(url.as_str())));
                }
            }
            ContextMenuCommand::ReplaceMisspelling { word } => {
                host.replace_misspelling(Some(&CefString::from(word.as_str())));
            }
            ContextMenuCommand::AddToDictionary { word } => {
                host.add_word_to_dictionary(Some(&CefString::from(word.as_str())));
            }
            ContextMenuCommand::ExtensionItem { ext_id, item_id } => {
                return Some(MenuFollowUp::Extension { ext_id, item_id, target: held.target });
            }
        }
        None
    }
}

fn allowed_menu_url(
    raw: &str,
    _roots: &[String],
    preview: Option<&Arc<crate::preview::PreviewGrant>>,
) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed.len() > crate::prompts::MAX_POPUP_URL_BYTES {
        return None;
    }
    if let Some(grant) = preview {
        if let Some(display) = grant.compute_display_url(trimmed) {
            return Some(display);
        }
        return browser_policy::destination_origin(trimmed)
            .is_some()
            .then(|| trimmed.to_owned());
    }
    browser_policy::destination_origin(trimmed)
        .is_some()
        .then(|| trimmed.to_owned())
}

wrap_context_menu_handler! {
    pub struct TabContextMenuHandler {
        tab_id: u64,
        shared: PromptShared,
        prompts: Arc<crate::prompts::TabPrompts>,
        file_roots: Arc<Mutex<Vec<String>>>,
        preview: Option<Arc<crate::preview::PreviewGrant>>,
        ext_menus: Option<Arc<Mutex<crate::extensions::context_menus::ExtensionContextMenuCatalog>>>,
        menu_state: Arc<TabMenuState>,
        out_tx: Output,
    }

    impl ContextMenuHandler {
        fn on_before_context_menu(
            &self,
            _browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            _params: Option<&mut ContextMenuParams>,
            model: Option<&mut MenuModel>,
        ) {
            // Strip Chromium's default menu (DevTools, View Source, Print, etc.), keeping one
            // sentinel entry so CEF invokes `run_context_menu` instead of suppressing it.
            if let Some(model) = model {
                model.clear();
                model.add_item(1, Some(&CefString::from("menu")));
            }
        }

        fn run_context_menu(
            &self,
            browser: Option<&mut Browser>,
            frame: Option<&mut Frame>,
            params: Option<&mut ContextMenuParams>,
            _model: Option<&mut MenuModel>,
            callback: Option<&mut RunContextMenuCallback>,
        ) -> ::std::os::raw::c_int {
            let Some(callback) = callback else { return 1 };
            // The agent's right-click (or a page's menu off the agent's input) never opens the app's
            // menu over the person's window: only after their own press or key (`chromium.ts:2500-2501`).
            if !self.shared.enabled.load(Ordering::Relaxed) || self.prompts.menu_suppressed.load(Ordering::Acquire) {
                callback.cancel();
                return 1;
            }
            let Some(params) = params else {
                callback.cancel();
                return 1;
            };
            let roots = self.file_roots.lock().unwrap().clone();
            let raw_link = CefString::from(&params.link_url()).to_string();
            let raw_src = CefString::from(&params.source_url()).to_string();
            let raw_page = CefString::from(&params.page_url()).to_string();
            let raw_frame = CefString::from(&params.frame_url()).to_string();
            let page_url = self.preview.as_ref().map_or(raw_page.clone(), |g| g.sanitize_reported_url(&raw_page));
            let frame_url = self.preview.as_ref().map_or(raw_frame.clone(), |g| g.sanitize_reported_url(&raw_frame));
            let selection_text = CefString::from(&params.selection_text()).to_string();
            let misspelled_raw = CefString::from(&params.misspelled_word()).to_string();
            let mut sugg_list = CefStringList::new();
            let suggestions = if !misspelled_raw.is_empty()
                && params.dictionary_suggestions(Some(&mut sugg_list)) != 0
            {
                sugg_list
                    .into_iter()
                    .take(5)
                    .map(|s| crate::prompts::sanitize_wire_line(&s, 128))
                    .collect()
            } else {
                Vec::new()
            };
            let edit = params.edit_state_flags().as_ref().0 as u32;
            let media_enum = *params.media_type().as_ref();
            let is_image = media_enum == sys::cef_context_menu_media_type_t::CM_MEDIATYPE_IMAGE;
            let media_type = match media_enum {
                sys::cef_context_menu_media_type_t::CM_MEDIATYPE_IMAGE => Some("image".to_owned()),
                sys::cef_context_menu_media_type_t::CM_MEDIATYPE_VIDEO => Some("video".to_owned()),
                sys::cef_context_menu_media_type_t::CM_MEDIATYPE_AUDIO => Some("audio".to_owned()),
                _ => None,
            };
            let id = self.shared.next_id();
            let clean_page_url = crate::prompts::sanitize_wire_line(&page_url, 4096);
            let clean_frame_url = crate::prompts::sanitize_wire_line(&frame_url, 4096);
            let link_url = allowed_menu_url(&raw_link, &roots, self.preview.as_ref())
                .filter(|u| u.len() <= crate::prompts::MAX_POPUP_URL_BYTES);
            let src_url = if is_image && self.preview.is_none() {
                allowed_menu_url(&raw_src, &roots, None)
                    .filter(|u| u.len() <= crate::prompts::MAX_POPUP_URL_BYTES)
            } else {
                None
            };
            let ext_src_url = if self.preview.is_none() {
                allowed_menu_url(&raw_src, &roots, None)
                    .filter(|u| u.len() <= crate::prompts::MAX_POPUP_URL_BYTES)
            } else {
                None
            };
            let is_editable = params.is_editable() != 0;
            let (spellcheck, languages, dictionaries) = browser
                .as_ref()
                .filter(|_| is_editable)
                .and_then(|b| b.host())
                .and_then(|h| h.request_context())
                .map(|ctx| crate::menu_extras::spellcheck_state(&ctx))
                .unwrap_or_default();
            let sel_capped: String = selection_text.chars().take(512).collect();
            let target = crate::extensions::context_menus::MenuTarget {
                page_url: clean_page_url.clone(),
                frame_url: clean_frame_url.clone(),
                link_url: link_url.clone(),
                src_url: ext_src_url,
                media_type,
                is_editable,
                selection_text: sel_capped.clone(),
            };
            let extensions = if self.preview.is_none()
                && !self.prompts.agent_driven.load(Ordering::Acquire)
                && let Some(catalog) = &self.ext_menus
            {
                catalog.lock().unwrap().build_groups(&target)
            } else {
                Vec::new()
            };
            let menu = PageContextMenu {
                id,
                x: params.xcoord(),
                y: params.ycoord(),
                page_url: clean_page_url,
                frame_url: clean_frame_url,
                link_url,
                // Never expose `src_url` / `SaveImage` on preview tabs (`notes/review-rd281.md` #5).
                src_url,
                is_image,
                has_image_contents: params.has_image_contents() != 0,
                is_editable,
                selection_text: sel_capped,
                can_cut: (edit & sys::cef_context_menu_edit_state_flags_t::CM_EDITFLAG_CAN_CUT.0 as u32) != 0,
                can_copy: (edit & sys::cef_context_menu_edit_state_flags_t::CM_EDITFLAG_CAN_COPY.0 as u32) != 0,
                can_paste: (edit & sys::cef_context_menu_edit_state_flags_t::CM_EDITFLAG_CAN_PASTE.0 as u32) != 0,
                can_select_all: (edit & sys::cef_context_menu_edit_state_flags_t::CM_EDITFLAG_CAN_SELECT_ALL.0 as u32) != 0,
                misspelled_word: (!misspelled_raw.is_empty())
                    .then(|| crate::prompts::sanitize_wire_line(&misspelled_raw, 256)),
                suggestions,
                extensions,
                spellcheck,
                languages,
                dictionaries,
            };
            let full_selection = crate::menu_extras::truncate_utf8(
                &selection_text,
                crate::menu_extras::MAX_COPIED_TEXT_BYTES,
            );
            if let Some(prev) = self.menu_state.held.lock().unwrap().replace(HeldMenu {
                id,
                src_url: menu.src_url.clone().filter(|_| menu.has_image_contents),
                target,
                editable: is_editable,
                selection_text: full_selection,
                can_copy: menu.can_copy,
                can_cut: menu.can_cut,
                frame_id: frame.map(|f| CefString::from(&f.identifier()).to_string()).filter(|s| !s.is_empty()),
                callback: callback.clone(),
            }) {
                prev.callback.cancel();
            }
            let _ = self.out_tx.send(EngineMsg::ContextMenu { tab_id: self.tab_id, menu });
            1
        }
    }
}
