//! Screenshot capture, console/network diagnostics, and tab-owned file list/get executors.

use std::io::Cursor;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver};

use base64::Engine;
use browser_pane::tools::diagnostics::TabDiagnostics;
use opencode_kit::browser_rpc::{
    Command, MAX_FILE_BYTES, Outcome, TargetMeta, WireFile, failure_code, is_valid_file_id,
    new_file_id,
};
use rune::Waker;
use serde_json::{Value, json};

use super::super::session::{CapturedFile, SessionAttachment};
use super::point::PointResolver;
use super::read::{queue_live_frame_refresh, step_live_frame_refresh};
use super::{ExecPhase, StepOutcome, cdp_err_msg, check_page_guards, send_rpc_cdp, send_rpc_cdp_in_frame, verify_authorized_state};
use crate::App;

const MAX_FULLPAGE_EDGE: f64 = 16_384.0;
const MAX_FULLPAGE_PIXELS: f64 = 16_000_000.0;
/// Maximum raw base64 characters accepted from `Page.captureScreenshot` before rejecting (16 MiB).
const MAX_RAW_SCREENSHOT_B64: usize = 16 * 1024 * 1024;
/// Cap concurrent off-UI screenshot decodes so 16 in-flight commands cannot amplify peak RAM.
const MAX_CONCURRENT_IMG_DECODES: usize = 2;
static ACTIVE_IMG_DECODES: AtomicUsize = AtomicUsize::new(0);

struct ImgDecodeGuard;

impl ImgDecodeGuard {
    fn try_acquire() -> Option<Self> {
        let mut cur = ACTIVE_IMG_DECODES.load(Ordering::Relaxed);
        loop {
            if cur >= MAX_CONCURRENT_IMG_DECODES {
                return None;
            }
            match ACTIVE_IMG_DECODES.compare_exchange_weak(
                cur,
                cur + 1,
                Ordering::AcqRel,
                Ordering::Relaxed,
            ) {
                Ok(_) => return Some(Self),
                Err(next) => cur = next,
            }
        }
    }
}

impl Drop for ImgDecodeGuard {
    fn drop(&mut self) {
        ACTIVE_IMG_DECODES.fetch_sub(1, Ordering::AcqRel);
    }
}

pub(super) struct ScreenshotState {
    pub host_id: u64,
    pub format: String,
    pub quality: Option<u64>,
    pub max_width: Option<u32>,
    pub expected_gen: u64,
    pub expected_target: Option<TargetMeta>,
    pub cmd: Command,
    pub stage: ScreenshotStage,
}

pub(super) struct NetworkGetState {
    pub host_id: u64,
    pub req_id: String,
    pub include_body: bool,
    pub include_resp: bool,
    pub max_body: usize,
    pub cdp_req: u64,
}

impl ScreenshotState {
    pub fn cancel_cleanup(&mut self, app: &mut App) {
        match &mut self.stage {
            ScreenshotStage::GetBox { cdp_req }
            | ScreenshotStage::PlaceRef { cdp_req, .. }
            | ScreenshotStage::GetMetrics { cdp_req }
            | ScreenshotStage::Capture { cdp_req } => {
                app.workspace_panes.browser.host.cancel_cdp(*cdp_req);
            }
            ScreenshotStage::Locate { resolver } => resolver.cancel_cleanup(app),
            ScreenshotStage::VerifyPost { pending, .. } => {
                for (req_id, _) in pending.drain(..) {
                    app.workspace_panes.browser.host.cancel_cdp(req_id);
                }
            }
            ScreenshotStage::Process { .. } => {}
        }
    }
}

pub(super) enum ScreenshotStage {
    GetBox { cdp_req: u64 },
    /// An element in a child frame: its box through every frame owner (official `rect`).
    Locate { resolver: Box<PointResolver> },
    /// That box, waiting for the page's scroll offset.
    PlaceRef { cdp_req: u64, rect: (f64, f64, f64, f64) },
    GetMetrics { cdp_req: u64 },
    Capture { cdp_req: u64 },
    Process { rx: Receiver<Result<Vec<u8>, String>>, resources: Vec<String> },
    VerifyPost { pending: Vec<(u64, Option<String>)>, bytes: Vec<u8> },
}

pub(super) fn try_start(
    att: &mut SessionAttachment,
    app: &mut App,
    cmd: &Command,
    _waker: Option<Waker>,
) -> Option<StepOutcome> {
    match cmd.action_type()? {
        "screenshot" => Some(start_screenshot(att, app, cmd)),
        "console" => Some(exec_console(att, app, cmd)),
        "network.list" => Some(exec_network_list(att, app, cmd)),
        "network.get" => Some(exec_network_get(att, app, cmd)),
        "files.list" => Some(exec_files_list(att, app, cmd)),
        "files.get" => Some(exec_files_get(att, app, cmd)),
        _ => None,
    }
}

fn start_screenshot(att: &mut SessionAttachment, app: &mut App, cmd: &Command) -> StepOutcome {
    let Some(wire_id) = cmd.tab_id() else {
        return StepOutcome::Done(Outcome::fail(failure_code::OPERATION_FAILED, "Missing tabID."));
    };
    let ref_arg = cmd.action.get("ref").and_then(Value::as_str);
    let full_page = cmd.action.get("fullPage").and_then(Value::as_bool).unwrap_or(false);
    if ref_arg.is_some() && full_page {
        return StepOutcome::Done(Outcome::fail(
            failure_code::OPERATION_FAILED,
            "Choose either ref for an element screenshot or fullPage:true for the whole page.",
        ));
    }
    let host_id = match att.resolve_tab(app, wire_id) {
        Ok(id) => id,
        Err(msg) => return StepOutcome::Done(Outcome::fail(failure_code::OPERATION_FAILED, msg)),
    };
    if let Err(o) = check_page_guards(app, host_id, "screenshot") {
        return StepOutcome::Done(o);
    }
    let format = cmd.action.get("format").and_then(Value::as_str).unwrap_or("png");
    if !matches!(format, "png" | "jpeg" | "webp") {
        return StepOutcome::Done(Outcome::fail(
            failure_code::OPERATION_FAILED,
            "Unsupported screenshot format. Use 'png', 'jpeg', or 'webp'.",
        ));
    }
    let format = format.to_owned();
    let quality = cmd.action.get("quality").and_then(Value::as_u64).map(|q| q.clamp(1, 100));
    let max_width = cmd
        .action
        .get("maxWidth")
        .and_then(Value::as_u64)
        .map(|v| (v as u32).clamp(100, 4_000));
    app.workspace_panes.browser.host.set_agent_driven(host_id, true);
    let expected_gen = cmd.generation.unwrap_or_else(|| att.generation_of(host_id));
    let expected_target = cmd.target.clone();

    if let Some(r) = ref_arg {
        let Some((bid, ref_session)) = att.refs.get(&host_id).and_then(|t| t.resolve(r)) else {
            return StepOutcome::Done(Outcome::fail(
                failure_code::STALE_REF,
                format!("Element ref {r} is stale or absent. Refresh the snapshot first."),
            ));
        };
        let ref_frame = att.refs.get(&host_id).and_then(|t| t.frame_of(r));
        if ref_session.is_some() || ref_frame.is_some() {
            return match PointResolver::start(att, app, host_id, (bid, ref_session, ref_frame)) {
                Ok(resolver) => StepOutcome::Pending(ExecPhase::Screenshot(ScreenshotState {
                    host_id,
                    format,
                    quality,
                    max_width,
                    expected_gen,
                    expected_target,
                    cmd: cmd.clone(),
                    stage: ScreenshotStage::Locate { resolver: Box::new(resolver) },
                })),
                Err(o) => StepOutcome::Done(o),
            };
        }
        match send_rpc_cdp_in_frame(
            app,
            host_id,
            "DOM.getBoxModel",
            json!({ "backendNodeId": bid }),
            ref_session,
            ref_frame,
        ) {
            Ok(cdp_req) => StepOutcome::Pending(ExecPhase::Screenshot(ScreenshotState {
                host_id,
                format,
                quality,
                max_width,
                expected_gen,
                expected_target,
                cmd: cmd.clone(),
                stage: ScreenshotStage::GetBox { cdp_req },
            })),
            Err(e) => StepOutcome::Done(Outcome::fail(failure_code::OPERATION_FAILED, e)),
        }
    } else if full_page {
        match send_rpc_cdp(app, host_id, "Page.getLayoutMetrics", json!({}), None) {
            Ok(cdp_req) => StepOutcome::Pending(ExecPhase::Screenshot(ScreenshotState {
                host_id,
                format,
                quality,
                max_width,
                expected_gen,
                expected_target,
                cmd: cmd.clone(),
                stage: ScreenshotStage::GetMetrics { cdp_req },
            })),
            Err(e) => StepOutcome::Done(Outcome::fail(failure_code::OPERATION_FAILED, e)),
        }
    } else {
        let mut params = json!({ "format": format });
        if let Some(q) = quality
            && format != "png"
        {
            params["quality"] = json!(q);
        }
        match send_rpc_cdp(app, host_id, "Page.captureScreenshot", params, None) {
            Ok(cdp_req) => StepOutcome::Pending(ExecPhase::Screenshot(ScreenshotState {
                host_id,
                format,
                quality,
                max_width,
                expected_gen,
                expected_target,
                cmd: cmd.clone(),
                stage: ScreenshotStage::Capture { cdp_req },
            })),
            Err(e) => StepOutcome::Done(Outcome::fail(failure_code::OPERATION_FAILED, e)),
        }
    }
}

fn exec_console(att: &mut SessionAttachment, app: &App, cmd: &Command) -> StepOutcome {
    let Some(wire_id) = cmd.tab_id() else {
        return StepOutcome::Done(Outcome::fail(failure_code::OPERATION_FAILED, "Missing tabID."));
    };
    let host_id = match att.resolve_tab(app, wire_id) {
        Ok(id) => id,
        Err(msg) => return StepOutcome::Done(Outcome::fail(failure_code::OPERATION_FAILED, msg)),
    };
    if let Err(o) = check_page_guards(app, host_id, "console") {
        return StepOutcome::Done(o);
    }
    let Some(tab) = att.wire_tab(app, host_id) else {
        return StepOutcome::Done(Outcome::fail(failure_code::OPERATION_FAILED, "Tab unavailable."));
    };
    let level = cmd.action.get("level").and_then(Value::as_str);
    let limit = cmd.action.get("limit").and_then(Value::as_u64).map(|v| v as usize);
    let mut val = att
        .diagnostics
        .get(&host_id)
        .map(|d| d.query_console(level, limit))
        .unwrap_or_else(|| json!({ "messages": [], "truncated": false, "dropped": 0 }));
    val["tab"] = serde_json::to_value(tab).unwrap_or(Value::Null);
    StepOutcome::Done(Outcome::ok(val, Vec::new()))
}

fn exec_network_list(att: &mut SessionAttachment, app: &App, cmd: &Command) -> StepOutcome {
    let Some(wire_id) = cmd.tab_id() else {
        return StepOutcome::Done(Outcome::fail(failure_code::OPERATION_FAILED, "Missing tabID."));
    };
    let host_id = match att.resolve_tab(app, wire_id) {
        Ok(id) => id,
        Err(msg) => return StepOutcome::Done(Outcome::fail(failure_code::OPERATION_FAILED, msg)),
    };
    if let Err(o) = check_page_guards(app, host_id, "network.list") {
        return StepOutcome::Done(o);
    }
    let Some(tab) = att.wire_tab(app, host_id) else {
        return StepOutcome::Done(Outcome::fail(failure_code::OPERATION_FAILED, "Tab unavailable."));
    };
    let url_contains = cmd.action.get("urlContains").and_then(Value::as_str);
    let rtype = cmd.action.get("resourceType").and_then(Value::as_str);
    let limit = cmd.action.get("limit").and_then(Value::as_u64).map(|v| v as usize);
    let mut val = att
        .diagnostics
        .get(&host_id)
        .map(|d| d.query_network_list(url_contains, rtype, limit))
        .unwrap_or_else(|| json!({ "requests": [], "truncated": false, "dropped": 0 }));
    val["tab"] = serde_json::to_value(tab).unwrap_or(Value::Null);
    StepOutcome::Done(Outcome::ok(val, Vec::new()))
}

fn exec_network_get(att: &mut SessionAttachment, app: &mut App, cmd: &Command) -> StepOutcome {
    let Some(wire_id) = cmd.tab_id() else {
        return StepOutcome::Done(Outcome::fail(failure_code::OPERATION_FAILED, "Missing tabID."));
    };
    let Some(req_id) = cmd.action.get("id").and_then(Value::as_str) else {
        return StepOutcome::Done(Outcome::fail(failure_code::OPERATION_FAILED, "Missing request id."));
    };
    let host_id = match att.resolve_tab(app, wire_id) {
        Ok(id) => id,
        Err(msg) => return StepOutcome::Done(Outcome::fail(failure_code::OPERATION_FAILED, msg)),
    };
    if let Err(o) = check_page_guards(app, host_id, "network.get") {
        return StepOutcome::Done(o);
    }
    let Some(tab) = att.wire_tab(app, host_id) else {
        return StepOutcome::Done(Outcome::fail(failure_code::OPERATION_FAILED, "Tab unavailable."));
    };
    let Some(hop) = att.diagnostics.get(&host_id).and_then(|d| d.network_hop(req_id)).cloned() else {
        return StepOutcome::Done(Outcome::fail(
            failure_code::OPERATION_FAILED,
            "Request ID is no longer retained in this tab or belongs to another tab. Call browser.network.list({tabID}) and copy a current id.",
        ));
    };
    let include_body = cmd.action.get("includeBody").and_then(Value::as_bool).unwrap_or(false);
    // Official `diagnostics.ts:287`: `includeResponseBody ?? includeBody`.
    let include_resp = cmd.action.get("includeResponseBody").and_then(Value::as_bool).unwrap_or(include_body);
    let max_body = cmd.action.get("maxBodyChars").and_then(Value::as_u64)
        .map_or(20_000, |v| (v as usize).clamp(1, 20_000));
    // The request body is only what Chromium captured inline (`maxPostDataSize`), never
    // `Network.getRequestPostData`, as official (`diagnostics-machine.ts:289-300`).
    let host = &mut app.workspace_panes.browser.host;
    if include_resp
        && hop.response_body_fetchable()
        && host.engine_agent_tools()
        && host.tab_open_in_engine(host_id)
        && let Ok(cdp_req) = host.send_typed_cdp(
            host_id,
            "Network.getResponseBody",
            json!({ "requestId": hop.native_id }),
            hop.session_id.clone(),
        )
    {
        return StepOutcome::Pending(ExecPhase::NetworkGet(NetworkGetState {
            host_id, req_id: req_id.to_owned(), include_body, include_resp, max_body, cdp_req,
        }));
    }
    let mut val = TabDiagnostics::format_network_get(&hop, include_body, include_resp, max_body, None);
    val["tab"] = serde_json::to_value(tab).unwrap_or(Value::Null);
    StepOutcome::Done(Outcome::ok(val, Vec::new()))
}

pub(super) fn step_network_get(
    att: &SessionAttachment,
    app: &mut App,
    st: &mut NetworkGetState,
) -> Option<Outcome> {
    let resp = app.workspace_panes.browser.host.take_cdp_response(st.cdp_req)?;
    let Some(tab) = att.wire_tab(app, st.host_id) else {
        return Some(Outcome::fail(failure_code::OPERATION_FAILED, "Tab unavailable."));
    };
    let Some(hop) = att.diagnostics.get(&st.host_id).and_then(|d| d.network_hop(&st.req_id)).cloned() else {
        return Some(Outcome::fail(failure_code::OPERATION_FAILED, "Request ID is no longer retained."));
    };
    let fetched = match resp {
        Ok(v) => {
            let raw = v.get("body").and_then(Value::as_str).unwrap_or("");
            let b64 = v.get("base64Encoded").and_then(Value::as_bool).unwrap_or(false);
            if b64 {
                base64::engine::general_purpose::STANDARD
                    .decode(raw)
                    .ok()
                    .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
                    .ok_or("backendUnavailable")
            } else {
                Ok(raw.to_owned())
            }
        }
        Err(_) => Err("backendUnavailable"),
    };
    let mut val = TabDiagnostics::format_network_get(&hop, st.include_body, st.include_resp, st.max_body, Some(fetched));
    val["tab"] = serde_json::to_value(tab).unwrap_or(Value::Null);
    Some(Outcome::ok(val, Vec::new()))
}

fn exec_files_list(att: &mut SessionAttachment, app: &App, cmd: &Command) -> StepOutcome {
    let Some(wire_id) = cmd.tab_id() else {
        return StepOutcome::Done(Outcome::fail(failure_code::OPERATION_FAILED, "Missing tabID."));
    };
    let host_id = match att.resolve_tab(app, wire_id) {
        Ok(id) => id,
        Err(msg) => return StepOutcome::Done(Outcome::fail(failure_code::OPERATION_FAILED, msg)),
    };
    if let Err(o) = check_page_guards(app, host_id, "files.list") {
        return StepOutcome::Done(o);
    }
    let Some(tab) = att.wire_tab(app, host_id) else {
        return StepOutcome::Done(Outcome::fail(failure_code::OPERATION_FAILED, "Tab unavailable."));
    };
    let mut files: Vec<Value> = att
        .captures
        .get(&host_id)
        .into_iter()
        .flatten()
        .map(|c| {
            json!({
                "id": c.id,
                "name": c.name,
                "mime": c.mime,
                "bytes": c.bytes.len(),
                "state": "completed",
            })
        })
        .collect();
    let engine_downloads: Vec<_> = app
        .workspace_panes
        .browser
        .host
        .tab(host_id)
        .map(|bt| bt.files.clone())
        .unwrap_or_default();
    for d in &engine_downloads {
        let wire_fid = att.wire_download_id(host_id, &d.id);
        let st = match d.state {
            browser_pane::DownloadState::Pending => "pending",
            browser_pane::DownloadState::Completed => "completed",
            browser_pane::DownloadState::Failed => "failed",
        };
        files.push(json!({
            "id": wire_fid,
            "name": d.name,
            "mime": d.mime,
            "bytes": d.bytes,
            "state": st,
        }));
    }
    StepOutcome::Done(Outcome::ok(json!({ "tab": tab, "files": files }), Vec::new()))
}

fn exec_files_get(att: &mut SessionAttachment, app: &App, cmd: &Command) -> StepOutcome {
    let Some(wire_id) = cmd.tab_id() else {
        return StepOutcome::Done(Outcome::fail(failure_code::OPERATION_FAILED, "Missing tabID."));
    };
    let Some(file_id) = cmd.action.get("fileID").and_then(Value::as_str) else {
        return StepOutcome::Done(Outcome::fail(failure_code::OPERATION_FAILED, "Missing fileID."));
    };
    if !is_valid_file_id(file_id) {
        return StepOutcome::Done(Outcome::fail(
            failure_code::OPERATION_FAILED,
            "Invalid fileID format. Call browser.files.list({tabID}) and pass an exact file_<uuid>.",
        ));
    }
    let host_id = match att.resolve_tab(app, wire_id) {
        Ok(id) => id,
        Err(msg) => return StepOutcome::Done(Outcome::fail(failure_code::OPERATION_FAILED, msg)),
    };
    if let Err(o) = check_page_guards(app, host_id, "files.get") {
        return StepOutcome::Done(o);
    }
    let Some(tab) = att.wire_tab(app, host_id) else {
        return StepOutcome::Done(Outcome::fail(failure_code::OPERATION_FAILED, "Tab unavailable."));
    };
    if let Some(cap) = att
        .captures
        .get(&host_id)
        .and_then(|list| list.iter().find(|c| c.id == file_id))
    {
        return match WireFile::from_bytes(cap.id.clone(), cap.name.clone(), cap.mime.clone(), &cap.bytes) {
            Ok(wf) => StepOutcome::Done(Outcome::ok(json!({ "tab": tab }), vec![wf])),
            Err(e) => StepOutcome::Done(Outcome::fail(failure_code::OPERATION_FAILED, e)),
        };
    }
    if let Some(engine_id) = att.resolve_download_engine_id(host_id, file_id)
        && let Some(bt) = app.workspace_panes.browser.host.tab(host_id)
        && let Some(df) = bt.files.iter().find(|f| f.id == engine_id)
    {
        if df.state != browser_pane::DownloadState::Completed {
            return StepOutcome::Done(Outcome::fail(
                failure_code::OPERATION_FAILED,
                "Download is not completed yet.",
            ));
        }
        let Some(verified) = app.workspace_panes.browser.host.download_path(host_id, df) else {
            return StepOutcome::Done(Outcome::fail(
                failure_code::OPERATION_FAILED,
                "Download path failed confinement verification.",
            ));
        };
        let max_bytes = opencode_kit::browser_rpc::MAX_FILE_BYTES as u64;
        let mut bytes = Vec::new();
        let read_res = std::fs::File::open(&verified).and_then(|f| {
            use std::io::Read as _;
            f.take(max_bytes + 1).read_to_end(&mut bytes)
        });
        if read_res.is_err() {
            return StepOutcome::Done(Outcome::fail(
                failure_code::OPERATION_FAILED,
                "Could not read verified download file.",
            ));
        }
        if bytes.len() as u64 > max_bytes {
            return StepOutcome::Done(Outcome::fail(
                failure_code::OPERATION_FAILED,
                "Download file exceeds the 5 MiB transfer limit.",
            ));
        }
        return match WireFile::from_bytes(file_id.to_owned(), df.name.clone(), df.mime.clone(), &bytes) {
            Ok(wf) => StepOutcome::Done(Outcome::ok(json!({ "tab": tab }), vec![wf])),
            Err(e) => StepOutcome::Done(Outcome::fail(failure_code::OPERATION_FAILED, e)),
        };
    }
    StepOutcome::Done(Outcome::fail(
        failure_code::OPERATION_FAILED,
        "File ID not found in this tab. Call browser.files.list({tabID}) to see owned files.",
    ))
}

pub(super) fn step_screenshot(
    att: &mut SessionAttachment,
    app: &mut App,
    st: &mut ScreenshotState,
    waker: Option<Waker>,
) -> Option<Outcome> {
    if let ScreenshotStage::Locate { resolver } = &mut st.stage {
        let rect = match resolver.step(app)? {
            Ok(_) => resolver.rect(),
            Err(o) => return Some(o),
        };
        return match send_rpc_cdp(app, st.host_id, "Page.getLayoutMetrics", json!({}), None) {
            Ok(cdp_req) => {
                st.stage = ScreenshotStage::PlaceRef { cdp_req, rect };
                None
            }
            Err(e) => Some(Outcome::fail(failure_code::OPERATION_FAILED, e)),
        };
    }
    match &st.stage {
        ScreenshotStage::Locate { .. } => None,
        ScreenshotStage::PlaceRef { cdp_req, rect } => {
            let (rect, resp) = (*rect, app.workspace_panes.browser.host.take_cdp_response(*cdp_req)?);
            let val = resp.unwrap_or_default();
            // Page coordinates, as official adds `cssLayoutViewport.pageX/pageY` (`chromium.ts` `emulatedScreenshot`).
            let page = |k: &str| val.get("cssLayoutViewport").and_then(|v| v.get(k)).and_then(Value::as_f64).unwrap_or(0.0);
            capture_clip(app, st, (rect.0 + page("pageX"), rect.1 + page("pageY"), rect.2, rect.3))
        }
        ScreenshotStage::GetBox { cdp_req } => {
            let resp = app.workspace_panes.browser.host.take_cdp_response(*cdp_req)?;
            let val = match resp {
                Ok(v) => v,
                Err(_) => {
                    return Some(Outcome::fail(
                        failure_code::STALE_REF,
                        "Could not read bounding box for element ref.",
                    ));
                }
            };
            let Some(rect) = box_model_rect(&val) else {
                return Some(Outcome::fail(
                    failure_code::OPERATION_FAILED,
                    "Element has zero visible size.",
                ));
            };
            capture_clip(app, st, rect)
        }
        ScreenshotStage::GetMetrics { cdp_req } => {
            let resp = app.workspace_panes.browser.host.take_cdp_response(*cdp_req)?;
            let val = match resp {
                Ok(v) => v,
                Err(e) => return Some(Outcome::fail(failure_code::OPERATION_FAILED, cdp_err_msg(&e))),
            };
            let rect = val.get("cssContentSize").or_else(|| val.get("contentSize"));
            let w = rect.and_then(|r| r.get("width")).and_then(Value::as_f64).unwrap_or(1280.0).max(1.0);
            let h = rect.and_then(|r| r.get("height")).and_then(Value::as_f64).unwrap_or(800.0).max(1.0);
            let mut scale = 1.0f64;
            if w > MAX_FULLPAGE_EDGE {
                scale = scale.min(MAX_FULLPAGE_EDGE / w);
            }
            if h > MAX_FULLPAGE_EDGE {
                scale = scale.min(MAX_FULLPAGE_EDGE / h);
            }
            if (w * scale) * (h * scale) > MAX_FULLPAGE_PIXELS {
                scale *= (MAX_FULLPAGE_PIXELS / ((w * scale) * (h * scale))).sqrt();
            }
            let mut params = json!({
                "format": st.format,
                "captureBeyondViewport": true,
                "clip": { "x": 0.0, "y": 0.0, "width": w, "height": h, "scale": scale }
            });
            if let Some(q) = st.quality
                && st.format != "png"
            {
                params["quality"] = json!(q);
            }
            match send_rpc_cdp(app, st.host_id, "Page.captureScreenshot", params, None) {
                Ok(req) => {
                    st.stage = ScreenshotStage::Capture { cdp_req: req };
                    None
                }
                Err(e) => Some(Outcome::fail(failure_code::OPERATION_FAILED, e)),
            }
        }
        ScreenshotStage::Capture { cdp_req } => {
            let resp = app.workspace_panes.browser.host.take_cdp_response(*cdp_req)?;
            // Re-verify generation and target right after capture so a redirect mid-capture
            // cannot leak pixels from a newly loaded denied origin.
            if let Err(o) = verify_authorized_state(
                att,
                app,
                &st.cmd,
                Some(st.expected_gen),
                st.expected_target.as_ref(),
            ) {
                return Some(o);
            }
            let val = match resp {
                Ok(v) => v,
                Err(e) => return Some(Outcome::fail(failure_code::OPERATION_FAILED, cdp_err_msg(&e))),
            };
            let Some(b64) = val.get("data").and_then(Value::as_str) else {
                return Some(Outcome::fail(
                    failure_code::OPERATION_FAILED,
                    "Screenshot response omitted image data.",
                ));
            };
            if b64.len() > MAX_RAW_SCREENSHOT_B64 {
                return Some(Outcome::fail(
                    failure_code::OPERATION_FAILED,
                    "Captured screenshot exceeds the raw size limit. Try format='jpeg' or a smaller maxWidth.",
                ));
            }
            let resources = match att.source_urls(app, st.host_id) {
                Ok(urls) => urls,
                Err(e) => return Some(Outcome::fail(failure_code::OPERATION_FAILED, e)),
            };
            let Some(guard) = ImgDecodeGuard::try_acquire() else {
                return Some(Outcome::fail(
                    failure_code::OPERATION_FAILED,
                    "Too many concurrent screenshot decodes in progress. Wait for an existing capture to finish and retry.",
                ));
            };
            let b64_owned = b64.to_owned();
            let format = st.format.clone();
            let max_width = st.max_width;
            let (tx, rx) = mpsc::channel();
            std::thread::Builder::new()
                .name("browser-rpc-img".into())
                .spawn(move || {
                    let _guard = guard;
                    let res = (|| -> Result<Vec<u8>, String> {
                        let mut bytes = base64::engine::general_purpose::STANDARD
                            .decode(b64_owned.trim())
                            .map_err(|_| "Screenshot image data was not valid base64.".to_owned())?;
                        if let Some(max_w) = max_width {
                            bytes = maybe_downscale(&bytes, &format, max_w);
                        }
                        if bytes.len() > MAX_FILE_BYTES {
                            return Err(
                                "Captured screenshot exceeds the 5 MiB transfer limit. Try format='jpeg' or a smaller maxWidth."
                                    .to_owned(),
                            );
                        }
                        Ok(bytes)
                    })();
                    let _ = tx.send(res);
                    if let Some(w) = waker {
                        w();
                    }
                })
                .ok();
            st.stage = ScreenshotStage::Process { rx, resources };
            None
        }
        ScreenshotStage::Process { rx, resources } => {
            let res = rx.try_recv().ok()?;
            let bytes = match res {
                Ok(b) => b,
                Err(msg) => return Some(Outcome::fail(failure_code::OPERATION_FAILED, msg)),
            };
            if st.expected_target.is_some() && app.workspace_panes.browser.host.tab_open_in_engine(st.host_id) {
                let pending = queue_live_frame_refresh(att, app, st.host_id);
                if !pending.is_empty() {
                    st.stage = ScreenshotStage::VerifyPost { pending, bytes };
                    return None;
                }
            }
            let resources = resources.clone();
            Some(finish_screenshot(att, app, st, bytes, resources))
        }
        ScreenshotStage::VerifyPost { .. } => {
            let ScreenshotStage::VerifyPost { mut pending, bytes } =
                std::mem::replace(&mut st.stage, ScreenshotStage::Capture { cdp_req: 0 })
            else {
                return None;
            };
            match step_live_frame_refresh(att, app, st.host_id, &mut pending, None) {
                Ok(false) => {
                    st.stage = ScreenshotStage::VerifyPost { pending, bytes };
                    return None;
                }
                Ok(true) => {}
                Err(msg) => return Some(Outcome::fail(failure_code::OPERATION_FAILED, msg)),
            }
            let resources = match att.source_urls(app, st.host_id) {
                Ok(urls) => urls,
                Err(e) => return Some(Outcome::fail(failure_code::OPERATION_FAILED, e)),
            };
            Some(finish_screenshot(att, app, st, bytes, resources))
        }
    }
}

fn finish_screenshot(
    att: &mut SessionAttachment,
    app: &App,
    st: &ScreenshotState,
    bytes: Vec<u8>,
    resources: Vec<String>,
) -> Outcome {
    if let Err(o) = verify_authorized_state(
        att,
        app,
        &st.cmd,
        Some(st.expected_gen),
        st.expected_target.as_ref(),
    ) {
        return o;
    }
    let file_id = new_file_id();
    let ext = if st.format == "jpeg" { "jpg" } else { st.format.as_str() };
    let name = format!("screenshot.{ext}");
    let mime = format!("image/{}", st.format);
    let wf = match WireFile::from_bytes(file_id.clone(), name.clone(), mime.clone(), &bytes) {
        Ok(w) => w,
        Err(e) => return Outcome::fail(failure_code::OPERATION_FAILED, e),
    };
    att.record_capture(st.host_id, CapturedFile { id: file_id, name, mime, bytes, resources });
    let Some(tab) = att.wire_tab(app, st.host_id) else {
        return Outcome::fail(failure_code::OPERATION_FAILED, "Tab unavailable.");
    };
    Outcome::ok(json!({ "tab": tab }), vec![wf])
}

fn maybe_downscale(bytes: &[u8], format: &str, max_w: u32) -> Vec<u8> {
    if !matches!(format, "png" | "jpeg") {
        return bytes.to_vec();
    }
    let Ok(img) = image::load_from_memory(bytes) else {
        return bytes.to_vec();
    };
    if img.width() <= max_w {
        return bytes.to_vec();
    }
    let new_h = ((img.height() as u64 * max_w as u64) / img.width().max(1) as u64).max(1) as u32;
    let resized = img.resize(max_w, new_h, image::imageops::FilterType::Triangle);
    let mut out = Cursor::new(Vec::new());
    let fmt = if format == "jpeg" {
        image::ImageFormat::Jpeg
    } else {
        image::ImageFormat::Png
    };
    if resized.write_to(&mut out, fmt).is_ok() {
        out.into_inner()
    } else {
        bytes.to_vec()
    }
}

/// An element box: drawn (finite, positive; an ancestor's `scale(0)` gives none) and within the
/// full-page pixel bounds, scaled down to fit.
fn capture_clip(app: &mut App, st: &mut ScreenshotState, (x, y, w, h): (f64, f64, f64, f64)) -> Option<Outcome> {
    if ![x, y, w, h].iter().all(|v| v.is_finite()) || w <= 0.0 || h <= 0.0 {
        return Some(Outcome::fail(failure_code::OPERATION_FAILED, "Element has zero visible size."));
    }
    let scale = (MAX_FULLPAGE_EDGE / w).min(MAX_FULLPAGE_EDGE / h).min((MAX_FULLPAGE_PIXELS / (w * h)).sqrt()).min(1.0);
    let mut params = json!({ "format": st.format, "clip": { "x": x, "y": y, "width": w, "height": h, "scale": scale } });
    if let Some(q) = st.quality
        && st.format != "png"
    {
        params["quality"] = json!(q);
    }
    match send_rpc_cdp(app, st.host_id, "Page.captureScreenshot", params, None) {
        Ok(req) => {
            st.stage = ScreenshotStage::Capture { cdp_req: req };
            None
        }
        Err(e) => Some(Outcome::fail(failure_code::OPERATION_FAILED, e)),
    }
}

fn box_model_rect(v: &Value) -> Option<(f64, f64, f64, f64)> {
    let border = v.get("model")?.get("border")?.as_array()?;
    if border.len() < 8 {
        return None;
    }
    let xs = [0, 2, 4, 6].map(|i| border[i].as_f64().unwrap_or(0.0));
    let ys = [1, 3, 5, 7].map(|i| border[i].as_f64().unwrap_or(0.0));
    let min_x = xs.into_iter().fold(f64::INFINITY, f64::min);
    let max_x = xs.into_iter().fold(f64::NEG_INFINITY, f64::max);
    let min_y = ys.into_iter().fold(f64::INFINITY, f64::min);
    let max_y = ys.into_iter().fold(f64::NEG_INFINITY, f64::max);
    let w = max_x - min_x;
    let h = max_y - min_y;
    (w > 0.0 && h > 0.0).then_some((min_x, min_y, w, h))
}
