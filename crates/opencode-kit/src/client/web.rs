//! The connection in a browser. Requests are synchronous `XMLHttpRequest`s (through
//! `ureq_web`), so the same calls work in the same places as natively; they run from `rune::task`'s main-thread tasks, between
//! frames, never inside one. The event stream is a streamed `fetch`, read asynchronously, so a
//! quiet stream costs the page nothing. A page may call a server only when the server's CORS
//! rules allow its origin (a local page on `http://localhost:*` or `http://127.0.0.1:*` always).

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::Sender;
use std::time::Duration;

use rune::Waker;
use rune::time::Instant;
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;

use super::worker::{CONNECT_TIMEOUT, Handshake, StreamEffect, StreamEvent, StreamState, stream_step};
use super::{Incoming, Server};
use crate::wire_error::{WireError, WireKind};

impl Server {
    /// A browser can't open a socket to ask; the first request says.
    pub fn reachable(&self) -> bool {
        true
    }

    /// Starts GETs of `paths` now, all at once, for this server's `get`s from task work to take
    /// up (`ureq_web::start_get`): a page's reads otherwise go one after another.
    pub fn start_gets(&self, paths: &[&str]) {
        let auth = self.auth.as_deref().map(|a| ("Authorization", a));
        for path in paths {
            ureq_web::start_get(&format!("{}{path}", self.base), auth.as_slice());
        }
    }

    /// Whether every GET of `paths` was started and its answer is in, so `get`s of them return
    /// at once, outside task work too (`ureq_web::started_in`).
    pub fn gets_in(&self, paths: &[&str]) -> bool {
        let auth = self.auth.as_deref().map(|a| ("Authorization", a));
        let urls: Vec<String> = paths.iter().map(|path| format!("{}{path}", self.base)).collect();
        ureq_web::started_in(&urls, auth.as_slice())
    }

    /// Drops what [`Server::start_gets`] began that no `get` took up.
    pub fn forget_gets(&self) {
        ureq_web::forget_started();
    }
}

static RETRY: AtomicU64 = AtomicU64::new(0);

pub(super) fn wake_reconnects() {
    RETRY.fetch_add(1, Ordering::Relaxed);
}

/// The event stream, reopened with backoff after each drop, until `inbox` closes: a task on the
/// page's event loop that waits on the network, never on the main thread.
pub(super) fn read_events(server: &Server, idle: Duration, inbox_tx: &Sender<Incoming>, wake: &Waker) {
    let (server, inbox_tx, wake) = (server.clone(), inbox_tx.clone(), wake.clone());
    wasm_bindgen_futures::spawn_local(async move {
        let mut state = StreamState::Initial;
        while !matches!(state, StreamState::Closed) {
            let generation = RETRY.load(Ordering::Relaxed);
            match events(&server, idle, &inbox_tx, &wake, &mut state).await {
                Ok(()) => return,
                Err(e) => {
                    for effect in stream_step(&mut state, StreamEvent::Failed { error: e, now: Instant::now() }) {
                        match effect {
                            StreamEffect::ReportDown { error, wake: news } => {
                                if inbox_tx.send(Incoming::StreamDown(error)).is_err() {
                                    stream_step(&mut state, StreamEvent::Closed);
                                    return;
                                }
                                if news {
                                    wake();
                                }
                            }
                            StreamEffect::Wait { delay_ms } => {
                                // Official `reconnectDelay = 1_000`, interruptible by `wake_reconnects`.
                                let until = Instant::now() + Duration::from_millis(delay_ms);
                                while Instant::now() < until && RETRY.load(Ordering::Relaxed) == generation {
                                    sleep(Duration::from_millis(100)).await;
                                }
                                stream_step(&mut state, StreamEvent::WaitElapsed);
                            }
                            StreamEffect::DeliverHandshake(_) | StreamEffect::DeliverFrame | StreamEffect::RejectHandshake(_) => {}
                        }
                    }
                }
            }
        }
    });
}

/// One open of `/api/event`: `Ok` when the inbox closed, else why the stream ended.
async fn events(server: &Server, idle: Duration, inbox: &Sender<Incoming>, wake: &Waker, state: &mut StreamState) -> Result<(), WireError> {
    let down = |kind: WireKind| WireError::new("GET", "/api/event", kind);
    let page = |what: &str| down(WireKind::Other(what.to_owned()));
    let window = web_sys::window().ok_or_else(|| page("no window"))?;
    let abort = web_sys::AbortController::new().map_err(|_| page("no AbortController"))?;
    let init = web_sys::RequestInit::new();
    init.set_method("GET");
    init.set_signal(Some(&abort.signal()));
    let headers = web_sys::Headers::new().map_err(|_| page("no Headers"))?;
    headers.set("Accept", "text/event-stream").ok();
    if let Some(a) = &server.auth {
        headers.set("Authorization", a).ok();
    }
    init.set_headers(&headers);
    let request = web_sys::Request::new_with_str_and_init(&format!("{}/api/event", server.base), &init).map_err(|_| page("bad request"))?;
    let response = with_idle(window.fetch_with_request(&request), CONNECT_TIMEOUT, &abort)
        .await
        .map_err(|e| down(e.unwrap_or_else(WireKind::Unreachable)))?;
    let response: web_sys::Response = response.dyn_into().map_err(|_| page("not a response"))?;
    if !response.ok() {
        return Err(down(WireKind::Status { status: response.status(), tag: None, message: None }));
    }
    let content_type = response.headers().get("content-type").ok().flatten().unwrap_or_default();
    if !content_type.trim_start().to_ascii_lowercase().starts_with("text/event-stream") {
        let what = super::http::kind_of(&content_type, false);
        return Err(down(WireKind::NotOpenCode { status: response.status(), what }));
    }
    let reader: web_sys::ReadableStreamDefaultReader =
        response.body().ok_or_else(|| page("no body"))?.get_reader().dyn_into().map_err(|_| page("no reader"))?;
    let mut pending: Vec<u8> = Vec::new();
    loop {
        let chunk = with_idle(reader.read(), idle, &abort).await.map_err(|e| down(e.unwrap_or_else(WireKind::Dropped)))?;
        if js_sys::Reflect::get(&chunk, &"done".into()).ok().and_then(|d| d.as_bool()).unwrap_or(true) {
            return Err(down(WireKind::Dropped("the stream ended".into())));
        }
        let data:js_sys::Uint8Array = js_sys::Reflect::get(&chunk,&"value".into()).map_err(|_|page("no event bytes"))?.unchecked_into();
        let limit = 32 << 20;
        if pending.len() + data.length() as usize > limit { abort.abort(); return Err(down(WireKind::TooLarge { limit:limit as u64, length:None })); }
        let bytes=data.to_vec();
        pending.extend_from_slice(&bytes);
        let mut woke = false;
        while let Some(end) = pending.iter().position(|&b| b == b'\n') {
            let line: Vec<u8> = pending.drain(..=end).collect();
            let line = String::from_utf8_lossy(&line);
            let Some(message) = super::event_line(line.trim_end_matches(['\n', '\r'])) else { continue };
            let connected = matches!(&message, Incoming::Event { ty, .. } if ty == "server.connected");
            let mut pending_frame = Some(message);
            for effect in stream_step(state, StreamEvent::Frame { connected, now: Instant::now() }) {
                match effect {
                    StreamEffect::DeliverHandshake(h) => {
                        let msg = match h {
                            Handshake::Opened => Incoming::Opened,
                            Handshake::Reconnected => Incoming::Reconnected,
                        };
                        if inbox.send(msg).is_err() {
                            stream_step(state, StreamEvent::Closed);
                            return Ok(());
                        }
                        woke = true;
                    }
                    StreamEffect::DeliverFrame => {
                        if let Some(msg) = pending_frame.take() {
                            if inbox.send(msg).is_err() {
                                stream_step(state, StreamEvent::Closed);
                                return Ok(());
                            }
                            woke = true;
                        }
                    }
                    StreamEffect::RejectHandshake(err) => return Err(err),
                    _ => {}
                }
            }
        }
        if woke {
            wake();
        }
    }
}

/// `future`, or a timeout (aborting the request) after `idle` without an answer: `Ok(TimedOut)`;
/// `Err` is what the browser said when the promise failed, for the caller to type.
async fn with_idle(promise: js_sys::Promise, idle: Duration, abort: &web_sys::AbortController) -> Result<JsValue, Result<WireKind, String>> {
    let mut handle = None;
    let timer = js_sys::Promise::new(&mut |resolve, _| {
        handle = web_sys::window().and_then(|w| w.set_timeout_with_callback_and_timeout_and_arguments_1(&resolve, idle.as_millis() as i32, &JsValue::from_str("idle")).ok());
    });
    let raced = js_sys::Promise::race(&js_sys::Array::of2(&promise, &timer));
    let answer = JsFuture::from(raced).await;
    // Every chunk of the event stream races a fresh timer: one left armed would wake the page
    // `idle` after each event for nothing.
    if let (Some(window), Some(h)) = (web_sys::window(), handle) {
        window.clear_timeout_with_handle(h);
    }
    match answer {
        Ok(v) if v.as_string().as_deref() == Some("idle") => {
            abort.abort();
            Err(Ok(WireKind::TimedOut))
        }
        Ok(v) => Ok(v),
        Err(e) => {
            abort.abort();
            Err(Err(e.as_string().or_else(|| js_sys::Reflect::get(&e, &"message".into()).ok().and_then(|m| m.as_string())).unwrap_or_else(|| "connection failed".into())))
        }
    }
}

async fn sleep(wait: Duration) {
    let promise = js_sys::Promise::new(&mut |resolve, _| {
        if let Some(window) = web_sys::window() {
            window.set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, wait.as_millis() as i32).ok();
        }
    });
    JsFuture::from(promise).await.ok();
}
