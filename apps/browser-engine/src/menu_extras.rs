//! Engine half of the page menu's Copy image and Spellcheck / Languages items (official
//! `page-menu.ts:102, 119-150`, `native-page-menu.ts` `copyImageAt`, `setSpellCheckerEnabled`,
//! `setSpellCheckerLanguages`). The app draws the items once `Ready.menu_extras` says the engine
//! runs them (#516).
//!
//! CEF has no `copyImageAt`, so Copy image takes three DevTools steps: the image's bytes from the
//! frame's resources (`Page.getResourceContent`, which needs no CORS), a fresh isolated world in
//! that frame, and there a PNG re-encode from a `Blob` (`createImageBitmap` + `OffscreenCanvas`),
//! so every format Chromium decodes reaches the app as PNG for its clipboard
//! (`EngineMsg::CopiedImage`). The isolated world keeps the page's own scripts from replacing the
//! functions the re-encode uses.

use std::sync::atomic::{AtomicI32, Ordering};

use cef::{
    CefString, ImplBrowser, ImplBrowserHost, ImplFrame, ImplListValue, ImplPreferenceManager,
    ImplValue, RequestContext,
};
use serde_json::{Value, json};

/// The largest image (as base64) the engine reads or sends: inside the wire's 32 MiB line bound.
pub const MAX_COPIED_IMAGE_B64: usize = 24 * 1024 * 1024;

/// The largest plain-text selection copied or pasted over IPC (4 MiB, well inside the 16 MiB CDP
/// and 32 MiB wire caps).
pub const MAX_COPIED_TEXT_BYTES: usize = 4 * 1024 * 1024;

const SPELLCHECK_PREF: &str = "browser.enable_spellchecking";
const DICTIONARIES_PREF: &str = "spellcheck.dictionaries";
const ACCEPT_LANGUAGES_PREF: &str = "intl.accept_languages";

fn string_list(ctx: &RequestContext, name: &str) -> Vec<String> {
    let Some(v) = ctx.preference(Some(&CefString::from(name))) else { return Vec::new() };
    if let Some(list) = v.list() {
        return (0..list.size())
            .map(|i| CefString::from(&list.string(i)).to_string())
            .filter(|s| !s.is_empty())
            .collect();
    }
    CefString::from(&v.string())
        .to_string()
        .split(',')
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
        .collect()
}

/// `(spellcheck on, enabled languages, offerable languages)` for an editable field's menu. The
/// offer is the enabled dictionaries plus the profile's accept-languages; empty on macOS, where
/// the system's checker picks languages itself (official offers none there either).
pub fn spellcheck_state(ctx: &RequestContext) -> (bool, Vec<String>, Vec<String>) {
    let on = ctx
        .preference(Some(&CefString::from(SPELLCHECK_PREF)))
        .is_none_or(|v| v.bool() != 0);
    let languages = string_list(ctx, DICTIONARIES_PREF);
    if cfg!(target_os = "macos") {
        return (on, languages, Vec::new());
    }
    let mut offer = languages.clone();
    for lang in string_list(ctx, ACCEPT_LANGUAGES_PREF) {
        if !offer.contains(&lang) {
            offer.push(lang);
        }
    }
    offer.truncate(32);
    (on, languages, offer)
}

fn set_pref(ctx: &RequestContext, name: &str, mut val: cef::Value) -> bool {
    let key = CefString::from(name);
    if ctx.can_set_preference(Some(&key)) == 0 {
        return false;
    }
    // `CefString::default()` maps to NULL in `cef-rs`, which `SetPreference` rejects.
    let mut err = CefString::from(" ");
    ctx.set_preference(Some(&key), Some(&mut val), Some(&mut err)) != 0
}

pub fn toggle_spellcheck(ctx: &RequestContext) {
    let (on, _, _) = spellcheck_state(ctx);
    if let Some(val) = cef::value_create() {
        val.set_bool(i32::from(!on));
        set_pref(ctx, SPELLCHECK_PREF, val);
    }
}

/// Adds or removes one language; refuses a tag the menu did not offer, and removing the last one
/// (official `page-menu.ts:143`).
pub fn toggle_language(ctx: &RequestContext, value: &str) {
    let (_, languages, offer) = spellcheck_state(ctx);
    if !offer.iter().any(|o| o == value) {
        return;
    }
    let next: Vec<String> = if languages.iter().any(|l| l == value) {
        languages.into_iter().filter(|l| l != value).collect()
    } else {
        languages.into_iter().chain([value.to_owned()]).collect()
    };
    if next.is_empty() {
        return;
    }
    let (Some(mut list), Some(val)) = (cef::list_value_create(), cef::value_create()) else { return };
    for (i, lang) in next.iter().enumerate() {
        list.set_string(i, Some(&CefString::from(lang.as_str())));
    }
    val.set_list(Some(&mut list));
    set_pref(ctx, DICTIONARIES_PREF, val);
}

/// Step 1 of Copy image: the image's bytes from the frame's resources.
pub fn resource_request(id: i32, frame_id: &str, url: &str) -> String {
    json!({ "id": id, "method": "Page.getResourceContent", "params": { "frameId": frame_id, "url": url } }).to_string()
}

/// The image's bytes (base64) from step 1's reply, when it found any that fit.
pub fn resource_base64(result: &Value) -> Option<String> {
    let content = result.get("content").and_then(Value::as_str)?;
    if content.is_empty() || content.len() > MAX_COPIED_IMAGE_B64 {
        return None;
    }
    Some(if result.get("base64Encoded").and_then(Value::as_bool) == Some(true) {
        content.to_owned()
    } else {
        base64::Engine::encode(&base64::engine::general_purpose::STANDARD, content.as_bytes())
    })
}

/// Step 2: a fresh isolated world in the image's frame, so page script cannot touch step 3.
pub fn world_request(id: i32, frame_id: &str) -> String {
    json!({ "id": id, "method": "Page.createIsolatedWorld", "params": { "frameId": frame_id, "worldName": "opencode-copy-image" } })
        .to_string()
}

/// Step 3: re-encode the bytes as PNG in that world, from step 2's reply.
pub fn encode_request(id: i32, world: &Value, b64: &str) -> Option<String> {
    let context_id = world.get("executionContextId").and_then(Value::as_i64)?;
    let b64_json = serde_json::to_string(b64).ok()?;
    let expression = format!(
        r#"(async () => {{
  const raw = atob({b64_json}); const bytes = new Uint8Array(raw.length);
  for (let i = 0; i < raw.length; i++) bytes[i] = raw.charCodeAt(i);
  let image;
  try {{ image = await createImageBitmap(new Blob([bytes])); }} catch {{
    // `createImageBitmap` takes no SVG from a Blob; an image element does.
    image = new Image(); const src = URL.createObjectURL(new Blob([bytes], {{ type: "image/svg+xml" }}));
    try {{ image.src = src; await image.decode(); }} finally {{ URL.revokeObjectURL(src); }}
  }}
  const width = image.naturalWidth || image.width, height = image.naturalHeight || image.height;
  // A tiny file can claim a huge size (an SVG's own width and height): refuse it before the canvas.
  if (!(width > 0 && height > 0 && width <= 8192 && height <= 8192 && width * height <= 16777216)) return null;
  const canvas = new OffscreenCanvas(width, height);
  canvas.getContext("2d").drawImage(image, 0, 0, width, height);
  const png = new Uint8Array(await (await canvas.convertToBlob({{ type: "image/png" }})).arrayBuffer());
  let s = ""; for (let i = 0; i < png.length; i += 32768) s += String.fromCharCode.apply(null, png.subarray(i, i + 32768));
  return btoa(s);
}})()"#
    );
    let msg = json!({
        "id": id,
        "method": "Runtime.evaluate",
        "params": { "expression": expression, "contextId": context_id, "awaitPromise": true, "returnByValue": true },
    });
    Some(msg.to_string())
}

/// The PNG (base64) step 3 produced, when it is one and fits the wire.
pub fn png_result(result: &Value) -> Option<String> {
    let png = result.get("result")?.get("value")?.as_str()?;
    (png.len() <= MAX_COPIED_IMAGE_B64 && png.starts_with("iVBORw0KGgo")).then(|| png.to_owned())
}

/// Truncates `s` to at most `max_bytes` on a UTF-8 character boundary.
pub fn truncate_utf8(s: &str, max_bytes: usize) -> String {
    if s.len() <= max_bytes {
        return s.to_owned();
    }
    let mut end = max_bytes;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    s[..end].to_owned()
}

/// Starts the 3-step Copy image DevTools pipeline for a held context menu.
pub(crate) fn start_copy_image(
    t: &crate::engine::ManagedTab,
    next_cdp_id: &AtomicI32,
    url: &str,
    frame_id: Option<String>,
) {
    // The menu's own frame when the engine knows it on the tab's session,
    // else the main frame (an image in an out-of-process frame is not read).
    let frame = {
        let ledger = t.cdp_ledger.lock().unwrap();
        frame_id.filter(|f| ledger.is_root_session_frame(f)).or_else(|| ledger.main_frame_id())
    };
    if let Some(frame) = frame {
        let msg_id = next_cdp_id.fetch_add(1, Ordering::Relaxed);
        t.cdp_id_map
            .lock()
            .unwrap()
            .insert(msg_id, crate::handlers::CdpPending::CopyImageContent { frame_id: frame.clone() });
        let raw = resource_request(msg_id, &frame, url);
        t.host.send_dev_tools_message(Some(raw.as_bytes()));
    }
}

/// Step 1 of host-bridged Copy/Cut: create an isolated world in the focused frame.
pub fn copy_selection_world_request(id: i32, frame_id: &str, session_id: Option<&str>) -> String {
    let mut msg = json!({
        "id": id,
        "method": "Page.createIsolatedWorld",
        "params": { "frameId": frame_id, "worldName": "opencode-clipboard" },
    });
    if let Some(sid) = session_id {
        msg["sessionId"] = Value::String(sid.to_owned());
    }
    msg.to_string()
}

/// Step 2 of host-bridged Copy/Cut: read the selected text (and whether it is editable/cuttable)
/// inside the isolated world without triggering CEF's OS clipboard or page `copy` listeners.
pub fn copy_selection_eval_request(id: i32, world: &Value, session_id: Option<&str>) -> Option<String> {
    let context_id = world.get("executionContextId").and_then(Value::as_i64)?;
    let expression = r#"(() => {
  let doc = document;
  let win = window;
  let root = doc;
  let active = doc.activeElement;
  for (let depth = 0; depth < 16 && active; depth++) {
    if (active.shadowRoot && active.shadowRoot.activeElement) {
      root = active.shadowRoot;
      active = active.shadowRoot.activeElement;
      continue;
    }
    if (active.tagName === "IFRAME") {
      try {
        if (active.contentDocument && active.contentWindow) {
          doc = active.contentDocument;
          win = active.contentWindow;
          root = doc;
          active = doc.activeElement;
          continue;
        }
      } catch {}
    }
    break;
  }
  const cap = (s) => (s.length > 4194304 ? s.slice(0, 4194304) : s);
  if (active && (active.tagName === "TEXTAREA" || active.tagName === "INPUT")) {
    const type = String(active.type || "").toLowerCase();
    if (type === "password") return { text: "", canCut: false };
    const start = typeof active.selectionStart === "number" ? active.selectionStart : 0;
    const end = typeof active.selectionEnd === "number" ? active.selectionEnd : 0;
    if (end > start) {
      return { text: cap(String(active.value || "").slice(start, end)), canCut: !active.readOnly && !active.disabled };
    }
  }
  let sel = (root && typeof root.getSelection === "function") ? root.getSelection() : null;
  if (!sel || !sel.toString()) sel = win.getSelection();
  const text = sel ? sel.toString() : "";
  if (!text) return { text: "", canCut: false };
  let canCut = doc.designMode === "on" || Boolean(active && active.isContentEditable);
  if (!canCut && sel && sel.rangeCount > 0) {
    for (const raw of [sel.anchorNode, sel.focusNode]) {
      let node = raw;
      if (node && node.nodeType === 3) node = node.parentElement;
      if (node && node.isContentEditable) { canCut = true; break; }
    }
  }
  return { text: cap(text), canCut };
})()"#;
    let mut msg = json!({
        "id": id,
        "method": "Runtime.evaluate",
        "params": { "expression": expression, "contextId": context_id, "returnByValue": true },
    });
    if let Some(sid) = session_id {
        msg["sessionId"] = Value::String(sid.to_owned());
    }
    Some(msg.to_string())
}

/// Parses `{ text, canCut }` returned by `copy_selection_eval_request`.
pub fn copy_selection_result(result: &Value) -> Option<(String, bool)> {
    let val = result.get("result")?.get("value")?;
    let text = val.get("text")?.as_str()?;
    let can_cut = val.get("canCut").and_then(Value::as_bool).unwrap_or(false);
    Some((truncate_utf8(text, MAX_COPIED_TEXT_BYTES), can_cut))
}

/// Starts host-bridged selection copy/cut on the focused frame (including attached OOPIFs).
pub(crate) fn start_copy_selection(t: &crate::engine::ManagedTab, next_cdp_id: &AtomicI32, cut: bool) {
    let focused_id = t
        .browser
        .focused_frame()
        .or_else(|| t.browser.main_frame())
        .map(|f| CefString::from(&f.identifier()).to_string())
        .filter(|s| !s.is_empty());
    let (frame_id, session_id) = {
        let ledger = t.cdp_ledger.lock().unwrap();
        if let Some(fid) = focused_id {
            if let Some(sid) = ledger.session_for_frame(&fid) {
                (Some(fid), Some(sid))
            } else if ledger.is_root_session_frame(&fid) {
                (Some(fid), None)
            } else {
                (ledger.main_frame_id(), None)
            }
        } else {
            (ledger.main_frame_id(), None)
        }
    };
    if let Some(frame_id) = frame_id {
        let msg_id = next_cdp_id.fetch_add(1, Ordering::Relaxed);
        t.cdp_id_map.lock().unwrap().insert(
            msg_id,
            crate::handlers::CdpPending::CopySelectionWorld { cut, session_id: session_id.clone() },
        );
        let raw = copy_selection_world_request(msg_id, &frame_id, session_id.as_deref());
        t.host.send_dev_tools_message(Some(raw.as_bytes()));
    }
}

/// Runs a standard Blink edit command (`HostMsg::EditCmd`) on the focused (or main) frame.
pub(crate) fn run_edit_cmd(t: &crate::engine::ManagedTab, cmd: &str) {
    let Some(frame) = t.browser.focused_frame().or_else(|| t.browser.main_frame()) else {
        return;
    };
    match cmd {
        "select_all" => frame.select_all(),
        "copy" => frame.copy(),
        "paste" => frame.paste(),
        "paste_plain" => frame.paste_and_match_style(),
        "cut" => frame.cut(),
        "undo" => frame.undo(),
        "redo" => frame.redo(),
        _ => {}
    }
}

/// Inserts plain text from the host clipboard into the focused widget without touching CEF's OS clipboard.
pub(crate) fn paste_host_text(t: &crate::engine::ManagedTab, text: &str) {
    if text.is_empty() {
        return;
    }
    let bounded = truncate_utf8(text, MAX_COPIED_TEXT_BYTES);
    let normalized = if bounded.contains('\r') {
        bounded.replace("\r\n", "\n").replace('\r', "\n")
    } else {
        bounded
    };
    t.host.ime_commit_text(Some(&CefString::from(normalized.as_str())), None, 0);
}
