//! Authenticated local loopback HTTP/1.1 + CONNECT + WebSocket proxy forwarding remote-server
//! browser traffic strictly through `experimental.browser/tunnel.{open,read,write,close}`
//! (`packages/plugin-browser/src/proxy.ts`, `packages/gui-plugins/src/browser/network.ts`).
//!
//! Security invariants:
//! - **Remote-only egress:** Never dials any destination on the user's machine (zero local sockets
//!   for user loopback `127.0.0.1`/`localhost`/`[::1]`, RFC 1918 private ranges, link-local
//!   `169.254.0.0/16`/`fe80::/10`, ULA `fc00::/7`, or the app's own OpenCode ports). Every
//!   accepted HTTP request and `CONNECT` tunnel dials exclusively via the remote server's
//!   `experimental.browser/tunnel.open` RPC so loopback targets resolve on the remote server.
//! - **Loopback IP literal bind & fresh 384-bit credentials:** Binds strictly to `127.0.0.1:0`
//!   (with `SO_EXCLUSIVEADDRUSE` on Windows) and OS-CSPRNG (`getrandom::fill`, fail-closed) 16 B
//!   username + 32 B password per proxy instance (never reused across recycled ports).
//! - **Mandatory auth & header stripping:** Constant-time `Proxy-Authorization` check on every
//!   request and every `CONNECT` (`407 Proxy Authentication Required` otherwise); strips
//!   `Proxy-Authorization`, `Transfer-Encoding`, and hop-by-hop headers before forwarding.
//! - **Pool isolation & active watchdog deadlines:** Unauthenticated sockets use a separate 64-slot
//!   pool (`MAX_UNAUTH_CONNS = 64`) with a short 3 s total-head deadline so unauthenticated floods
//!   cannot starve the 64 authenticated tunnel slots and recover quickly. Established authenticated
//!   tunnels and response streams have NO idle kill (matching Official `proxy.ts`), while a
//!   `Condvar` deadline watchdog actively shuts down (`Shutdown::Both`) and aborts expired pre-auth
//!   heads (after a 1 s margin for writing `408`) and post-EOF sockets (~2 s after remote EOF for
//!   stalled POST bodies and half-open CONNECT/WS clients).

use std::collections::BTreeMap;
use std::io::{BufReader, Write};
use std::net::{Ipv4Addr, Shutdown, SocketAddrV4, TcpListener, TcpStream};
use std::sync::{Arc, Condvar, Mutex, PoisonError};
use std::thread::JoinHandle;
use std::time::Duration;

use base64::Engine;
use rune::time::Instant;
use socket2::{Domain, Protocol, Socket, Type};

use super::super::framing::{
    HeadReadErr, build_forwarded_head, constant_time_eq, parse_connect_authority,
    parse_content_length, parse_proxy_url, read_http_head, write_502_body, write_status,
};
use super::super::http::AbortHandle;
use super::super::proxy_machine::{ProxyEffect, ProxyEvent, ProxyState};
use super::pool::{ForwardOutcome, OriginPool, forward_http_pooled};
use super::pump::{
    WsHandshake, await_ws_upgrade, drain_remote_to_client, pump_bidirectional_stream,
};
use super::stream::TransportController;
use super::super::types::{LocationRef, MAX_TUNNELS, MuxCloseInfo, TunnelTarget, try_random_hex};
use crate::client::Server;

pub const PROXY_REALM: &str = "Brocode Browser Proxy";
const HEADER_TIMEOUT: Duration = Duration::from_secs(3);
const KEEP_ALIVE_IDLE_TIMEOUT: Duration = Duration::from_secs(30);
const WATCHDOG_HEAD_MARGIN: Duration = Duration::from_secs(1);
pub(super) const MAX_WS_RESP_HEAD: usize = 64 * 1024;
pub(super) const POST_EOF_DRAIN_TIMEOUT: Duration = Duration::from_secs(2);
const CLIENT_POLL_SLICE: Duration = Duration::from_millis(500);
const MAX_UNAUTH_CONNS: usize = MAX_TUNNELS;
const MAX_CONCURRENT_CONNS: usize = MAX_TUNNELS;

/// Binds an exclusive loopback TCP listener on `127.0.0.1:0`. On Windows, sets
/// `SO_EXCLUSIVEADDRUSE` before `bind` so no same-user process can co-bind/hijack via `SO_REUSEADDR`.
fn bind_exclusive_loopback() -> std::io::Result<TcpListener> {
    let sock = Socket::new(Domain::IPV4, Type::STREAM, Some(Protocol::TCP))?;
    #[cfg(windows)]
    {
        use std::os::windows::io::AsRawSocket;
        use windows_sys::Win32::Networking::WinSock::{SO_EXCLUSIVEADDRUSE, SOL_SOCKET, setsockopt};
        let one: i32 = 1;
        #[allow(unsafe_code)]
        // SAFETY: `sock` is a valid open Winsock socket and `&one` points to 4 readable bytes.
        let rc = unsafe {
            setsockopt(
                sock.as_raw_socket() as _,
                SOL_SOCKET,
                SO_EXCLUSIVEADDRUSE,
                (&one as *const i32).cast::<u8>(),
                std::mem::size_of::<i32>() as i32,
            )
        };
        if rc != 0 {
            return Err(std::io::Error::last_os_error());
        }
    }
    sock.bind(&SocketAddrV4::new(Ipv4Addr::LOCALHOST, 0).into())?;
    sock.listen(128)?;
    Ok(TcpListener::from(sock))
}

#[derive(Clone)]
pub struct ProxyCredentials {
    pub username: String,
    pub password: String,
}

struct TrackedClient {
    stream: TcpStream,
    deadline: Option<Instant>,
    expired: bool,
    aborts: Vec<AbortHandle>,
}

struct SharedProxy {
    state: ProxyState,
    suspended: bool,
    next_id: u64,
    unauth_clients: BTreeMap<u64, TrackedClient>,
    auth_clients: BTreeMap<u64, TrackedClient>,
}

impl SharedProxy {
    fn drain_all(&mut self) -> (Vec<TcpStream>, Vec<AbortHandle>) {
        let mut streams = Vec::with_capacity(self.unauth_clients.len() + self.auth_clients.len());
        let mut aborts = Vec::new();
        for c in std::mem::take(&mut self.unauth_clients)
            .into_values()
            .chain(std::mem::take(&mut self.auth_clients).into_values())
        {
            streams.push(c.stream);
            aborts.extend(c.aborts);
        }
        (streams, aborts)
    }

    fn client_mut(&mut self, id: u64) -> Option<&mut TrackedClient> {
        self.auth_clients.get_mut(&id).or_else(|| self.unauth_clients.get_mut(&id))
    }
}

/// Running loopback tunnel proxy for one remote-server browser attachment.
pub struct TunnelProxy {
    pub host: &'static str,
    pub port: u16,
    pub url: String,
    pub credentials: ProxyCredentials,
    transport: Arc<TransportController>,
    pool: Arc<OriginPool>,
    shared: Arc<Mutex<SharedProxy>>,
    watchdog_cv: Arc<Condvar>,
    listener_addr: std::net::SocketAddr,
    accept_thread: Option<JoinHandle<()>>,
    watchdog_thread: Option<JoinHandle<()>>,
}

impl TunnelProxy {
    /// Starts an authenticated loopback proxy bound strictly to IP literal `127.0.0.1:0` with
    /// fresh 384-bit OS-CSPRNG credentials, forwarding all traffic through `server`'s browser
    /// tunnel RPCs for `(location, session_id, connection_id)`.
    pub fn start(
        server: Server,
        location: LocationRef,
        session_id: String,
        connection_id: String,
    ) -> Result<Self, String> {
        let username = try_random_hex(16)?;
        let password = try_random_hex(32)?;
        let listener = bind_exclusive_loopback()
            .map_err(|e| format!("Failed to bind browser tunnel proxy: {e}"))?;
        let listener_addr = listener
            .local_addr()
            .map_err(|e| format!("Failed to read browser tunnel proxy address: {e}"))?;
        let expected_auth = format!(
            "Basic {}",
            base64::engine::general_purpose::STANDARD.encode(format!("{username}:{password}"))
        );
        let transport = Arc::new(TransportController::new(server, location, session_id, connection_id));
        let pool = Arc::new(OriginPool::default());
        let shared = Arc::new(Mutex::new(SharedProxy {
            state: ProxyState::Open,
            suspended: false,
            next_id: 1,
            unauth_clients: BTreeMap::new(),
            auth_clients: BTreeMap::new(),
        }));
        let watchdog_cv = Arc::new(Condvar::new());
        let ctx = Arc::new(ProxyCtx {
            transport: transport.clone(),
            pool: pool.clone(),
            expected_auth: expected_auth.into_bytes(),
            shared: shared.clone(),
            watchdog_cv: watchdog_cv.clone(),
        });
        let wd_shared = shared.clone();
        let wd_cv = watchdog_cv.clone();
        let watchdog_thread = std::thread::Builder::new()
            .name("browser-tunnel-watchdog".into())
            .spawn(move || watchdog_loop(wd_shared, wd_cv))
            .map_err(|e| format!("Failed to spawn browser tunnel watchdog: {e}"))?;
        let accept_thread = std::thread::Builder::new()
            .name("browser-tunnel-proxy".into())
            .spawn(move || accept_loop(listener, ctx))
            .map_err(|e| format!("Failed to spawn browser tunnel proxy thread: {e}"))?;
        let port = listener_addr.port();
        Ok(Self {
            host: "127.0.0.1",
            port,
            url: format!("http://127.0.0.1:{port}"),
            credentials: ProxyCredentials { username, password },
            transport,
            pool,
            shared,
            watchdog_cv,
            listener_addr,
            accept_thread: Some(accept_thread),
            watchdog_thread: Some(watchdog_thread),
        })
    }

    /// Activates negotiated transport once `attached` arrives: prewarms binary WebSocket `ClientMux`
    /// when `tunnel.ws.v1` is advertised (unless `OPENCODE_BROWSER_TUNNEL_TRANSPORT=legacy`).
    pub fn activate(
        &self,
        capabilities: &[String],
        on_stream_ready: Box<dyn FnOnce() + Send + 'static>,
        on_stream_closed: Box<dyn FnOnce(MuxCloseInfo) + Send + 'static>,
    ) {
        let pool = self.pool.clone();
        self.transport.activate(
            capabilities,
            on_stream_ready,
            on_stream_closed,
            Arc::new(move || pool.clear()),
        );
    }

    /// Issues `experimental.browser/tunnel.grant` for a trusted desktop navigation target when the
    /// server advertised `tunnel.ws.v1`; no-op in legacy mode.
    pub fn grant_private_target(&self, target: TunnelTarget, extra_ports: Vec<u16>) {
        self.transport.grant_private_target(target, extra_ports);
    }

    /// Immediately evicts and closes all idle pooled HTTP/1.1 connections.
    pub fn reset_pools(&self) {
        self.pool.clear();
    }

    /// Whether a prewarmed binary WebSocket multiplexer currently owns attachment liveness.
    pub fn stream_active(&self) -> bool {
        self.transport.stream_active()
    }

    /// Temporarily suspends or resumes forwarding without closing the listener port (used during
    /// transient remote reconnect so requests fail closed with `503` without tearing down tabs).
    pub fn set_suspended(&self, suspended: bool) {
        let (clients, aborts) = {
            let mut g = self.shared.lock().unwrap_or_else(PoisonError::into_inner);
            if g.suspended == suspended {
                return;
            }
            g.suspended = suspended;
            if suspended { g.drain_all() } else { (Vec::new(), Vec::new()) }
        };
        if suspended {
            self.pool.clear();
            self.watchdog_cv.notify_all();
        }
        for abort in aborts {
            abort.abort();
        }
        for stream in clients {
            stream.shutdown(Shutdown::Both).ok();
        }
    }

    /// Closes the proxy immediately: stops accepting, aborts all in-flight `tunnel.{open,read,write}`
    /// HTTP calls, shuts down every live client socket, and joins accept and watchdog threads.
    pub fn close(&mut self) {
        self.transport.dispose();
        self.pool.close();
        let (clients, aborts, effect) = {
            let mut g = self.shared.lock().unwrap_or_else(PoisonError::into_inner);
            let effect = g.state.step(ProxyEvent::Close);
            let (clients, aborts) = g.drain_all();
            (clients, aborts, effect)
        };
        self.watchdog_cv.notify_all();
        if effect == Some(ProxyEffect::Shutdown) {
            for abort in aborts {
                abort.abort();
            }
            for stream in clients {
                stream.shutdown(Shutdown::Both).ok();
            }
            // Wake blocking `TcpListener::incoming()` so the accept thread exits cleanly.
            let _ = TcpStream::connect_timeout(&self.listener_addr, Duration::from_millis(200));
        }
        if let Some(handle) = self.accept_thread.take() {
            handle.join().ok();
        }
        if let Some(handle) = self.watchdog_thread.take() {
            handle.join().ok();
        }
    }
}

impl Drop for TunnelProxy {
    fn drop(&mut self) {
        self.close();
    }
}

fn watchdog_loop(shared: Arc<Mutex<SharedProxy>>, cv: Arc<Condvar>) {
    let mut g = shared.lock().unwrap_or_else(PoisonError::into_inner);
    loop {
        if !g.state.admits() {
            break;
        }
        let now = Instant::now();
        let mut kill_streams = Vec::new();
        let mut kill_aborts = Vec::new();
        let mut next_deadline: Option<Instant> = None;
        let SharedProxy { unauth_clients, auth_clients, .. } = &mut *g;
        for c in unauth_clients.values_mut().chain(auth_clients.values_mut()) {
            if c.expired {
                continue;
            }
            let Some(deadline) = c.deadline else {
                continue;
            };
            if deadline <= now {
                c.expired = true;
                if let Ok(s) = c.stream.try_clone() {
                    kill_streams.push(s);
                }
                kill_aborts.extend(c.aborts.iter().cloned());
            } else {
                next_deadline = Some(next_deadline.map_or(deadline, |cur| cur.min(deadline)));
            }
        }
        if !kill_streams.is_empty() || !kill_aborts.is_empty() {
            drop(g);
            for abort in kill_aborts {
                abort.abort();
            }
            for stream in kill_streams {
                stream.shutdown(Shutdown::Both).ok();
            }
            g = shared.lock().unwrap_or_else(PoisonError::into_inner);
            continue;
        }
        g = match next_deadline {
            Some(at) => {
                let wait = at.saturating_duration_since(Instant::now()).max(Duration::from_millis(5));
                cv.wait_timeout(g, wait).unwrap_or_else(PoisonError::into_inner).0
            }
            None => cv.wait(g).unwrap_or_else(PoisonError::into_inner),
        };
    }
}

pub(super) struct ProxyCtx {
    pub(super) transport: Arc<TransportController>,
    pub(super) pool: Arc<OriginPool>,
    expected_auth: Vec<u8>,
    shared: Arc<Mutex<SharedProxy>>,
    watchdog_cv: Arc<Condvar>,
}

struct ConnGuard {
    ctx: Arc<ProxyCtx>,
    client_id: u64,
    authenticated: bool,
}

impl ConnGuard {
    fn promote_authenticated(&mut self) -> Result<(), AdmitError> {
        let mut g = self.ctx.shared.lock().unwrap_or_else(PoisonError::into_inner);
        if !g.state.admits() {
            return Err(AdmitError::Closed);
        }
        if g.suspended {
            return Err(AdmitError::Suspended);
        }
        let Some(mut entry) = g.unauth_clients.remove(&self.client_id) else {
            return Err(AdmitError::Closed);
        };
        if entry.expired {
            return Err(AdmitError::Closed);
        }
        if g.auth_clients.len() >= MAX_CONCURRENT_CONNS {
            return Err(AdmitError::AtCapacity);
        }
        // Established authenticated tunnels and response streams have NO idle kill (matching
        // Official `proxy.ts`); watchdog only arms again on remote EOF (`arm_post_eof`).
        entry.deadline = None;
        g.auth_clients.insert(self.client_id, entry);
        self.authenticated = true;
        Ok(())
    }

    fn track_abort(&mut self, abort: &AbortHandle) -> bool {
        let mut g = self.ctx.shared.lock().unwrap_or_else(PoisonError::into_inner);
        if !g.state.admits() || g.suspended {
            abort.abort();
            return false;
        }
        let Some(entry) = g.client_mut(self.client_id) else {
            abort.abort();
            return false;
        };
        if entry.expired {
            abort.abort();
            return false;
        }
        entry.aborts.push(abort.clone());
        true
    }

    fn reset_aborts(&mut self) {
        let mut g = self.ctx.shared.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(entry) = g.client_mut(self.client_id) {
            entry.aborts.clear();
        }
    }
}

impl Drop for ConnGuard {
    fn drop(&mut self) {
        let mut g = self.ctx.shared.lock().unwrap_or_else(PoisonError::into_inner);
        if self.authenticated {
            g.auth_clients.remove(&self.client_id);
        } else {
            g.unauth_clients.remove(&self.client_id);
        }
    }
}

enum AdmitError {
    Closed,
    AtCapacity,
    Suspended,
}

impl ProxyCtx {
    pub(super) fn admits(&self) -> bool {
        let g = self.shared.lock().unwrap_or_else(PoisonError::into_inner);
        g.state.admits() && !g.suspended
    }

    fn authorized(&self, header: Option<&str>) -> bool {
        let Some(value) = header else { return false };
        constant_time_eq(value.trim().as_bytes(), &self.expected_auth)
    }

    fn track_unauth(self: &Arc<Self>, stream: &TcpStream) -> Result<ConnGuard, AdmitError> {
        let mut g = self.shared.lock().unwrap_or_else(PoisonError::into_inner);
        if !g.state.admits() {
            return Err(AdmitError::Closed);
        }
        if g.suspended {
            return Err(AdmitError::Suspended);
        }
        if g.unauth_clients.len() >= MAX_UNAUTH_CONNS {
            return Err(AdmitError::AtCapacity);
        }
        let id = g.next_id;
        g.next_id = g.next_id.saturating_add(1);
        let cloned = stream.try_clone().map_err(|_| AdmitError::Closed)?;
        g.unauth_clients.insert(
            id,
            TrackedClient {
                stream: cloned,
                // Give `read_http_head` a 1 s margin after `HEADER_TIMEOUT` to write `408` cleanly
                // before the watchdog forcibly shuts the socket down.
                deadline: Some(Instant::now() + HEADER_TIMEOUT + WATCHDOG_HEAD_MARGIN),
                expired: false,
                aborts: Vec::new(),
            },
        );
        drop(g);
        self.watchdog_cv.notify_one();
        Ok(ConnGuard { ctx: self.clone(), client_id: id, authenticated: false })
    }

    pub(super) fn arm_post_eof(&self, client_id: u64, timeout: Duration) {
        let mut g = self.shared.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(c) = g.auth_clients.get_mut(&client_id)
            && !c.expired
        {
            let next = Instant::now() + timeout;
            c.deadline = Some(c.deadline.map_or(next, |cur| cur.min(next)));
            drop(g);
            self.watchdog_cv.notify_one();
        }
    }

    pub(super) fn is_expired(&self, client_id: u64) -> bool {
        let g = self.shared.lock().unwrap_or_else(PoisonError::into_inner);
        if !g.state.admits() || g.suspended {
            return true;
        }
        match g.auth_clients.get(&client_id) {
            Some(c) => c.expired || c.deadline.is_some_and(|dl| Instant::now() >= dl),
            None => true,
        }
    }
}

fn accept_loop(listener: TcpListener, ctx: Arc<ProxyCtx>) {
    for incoming in listener.incoming() {
        let Ok(mut stream) = incoming else { break };
        match ctx.track_unauth(&stream) {
            Ok(guard) => {
                let ctx = ctx.clone();
                std::thread::Builder::new()
                    .name("browser-tunnel-conn".into())
                    .spawn(move || handle_client(stream, ctx, guard))
                    .ok();
            }
            Err(AdmitError::AtCapacity | AdmitError::Suspended) => {
                write_status(&mut stream, "503 Service Unavailable").ok();
                stream.shutdown(Shutdown::Write).ok();
            }
            Err(AdmitError::Closed) => {
                stream.shutdown(Shutdown::Both).ok();
                break;
            }
        }
    }
}

fn handle_client(stream: TcpStream, ctx: Arc<ProxyCtx>, mut guard: ConnGuard) {
    stream.set_nodelay(true).ok();
    let mut reader = BufReader::new(stream);
    let mut keep_alive_iter = 0u32;

    loop {
        let timeout = if keep_alive_iter == 0 { HEADER_TIMEOUT } else { KEEP_ALIVE_IDLE_TIMEOUT };
        let head_deadline = Instant::now() + timeout;
        reader.get_ref().set_read_timeout(Some(timeout)).ok();
        let req = match read_http_head(&mut reader, head_deadline) {
            Ok(r) => r,
            Err(HeadReadErr::CleanEof) => return,
            Err(HeadReadErr::Status(status)) => {
                write_status(reader.get_mut(), status).ok();
                return;
            }
        };
        if !ctx.authorized(req.header("proxy-authorization")) {
            let resp = format!(
                "HTTP/1.1 407 Proxy Authentication Required\r\nProxy-Authenticate: Basic realm=\"{PROXY_REALM}\"\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
            );
            reader.get_mut().write_all(resp.as_bytes()).ok();
            return;
        }
        if !guard.authenticated {
            match guard.promote_authenticated() {
                Ok(()) => {}
                Err(AdmitError::AtCapacity | AdmitError::Suspended) => {
                    write_status(reader.get_mut(), "503 Service Unavailable").ok();
                    return;
                }
                Err(AdmitError::Closed) => return,
            }
        } else if !ctx.admits() {
            write_status(reader.get_mut(), "503 Service Unavailable").ok();
            return;
        } else {
            guard.reset_aborts();
        }

        let is_connect = req.method.eq_ignore_ascii_case("CONNECT");
        let upgrade_hdr = req.header("upgrade");
        let is_ws = !is_connect && upgrade_hdr.is_some_and(|v| v.eq_ignore_ascii_case("websocket"));
        if is_ws && (!req.method.eq_ignore_ascii_case("GET") || req.version != "HTTP/1.1") {
            write_status(reader.get_mut(), "400 Bad Request").ok();
            return;
        }
        if !is_connect && upgrade_hdr.is_some() && !is_ws {
            write_status(reader.get_mut(), "400 Bad Request").ok();
            return;
        }
        if req.has_header("transfer-encoding") {
            write_status(reader.get_mut(), "501 Not Implemented").ok();
            return;
        }
        if req.has_header("expect") {
            write_status(reader.get_mut(), "417 Expectation Failed").ok();
            return;
        }
        let content_length = match parse_content_length(&req, is_connect || is_ws) {
            Ok(n) => n,
            Err(status) => {
                write_status(reader.get_mut(), status).ok();
                return;
            }
        };
        if (is_connect || is_ws) && !reader.buffer().is_empty() {
            write_status(reader.get_mut(), "400 Bad Request").ok();
            return;
        }

        let parsed = if is_connect {
            parse_connect_authority(&req.target)
        } else {
            parse_proxy_url(&req.target).filter(|u| {
                if is_ws { matches!(u.scheme.as_str(), "http" | "ws") } else { u.scheme == "http" }
            })
        };
        let Some(url) = parsed else {
            write_status(reader.get_mut(), "400 Bad Request").ok();
            return;
        };
        let Some(target) = TunnelTarget::parse(&url.host, url.port) else {
            write_status(reader.get_mut(), "400 Bad Request").ok();
            return;
        };

        let client_id = guard.client_id;
        let io_abort = AbortHandle::new();
        if !guard.track_abort(&io_abort) {
            return;
        }

        if !is_connect && !is_ws {
            let prebuffered = reader.buffer().to_vec();
            let had_pipelined = prebuffered.len() as u64 > content_length;
            let mut client = reader.into_inner();
            client.set_read_timeout(Some(CLIENT_POLL_SLICE)).ok();
            match forward_http_pooled(
                &ctx,
                client_id,
                &req,
                &url,
                &target,
                content_length,
                &prebuffered,
                &mut client,
                &io_abort,
            ) {
                ForwardOutcome::Done { keep_client_alive }
                    if keep_client_alive && !had_pipelined && ctx.admits() =>
                {
                    keep_alive_iter = keep_alive_iter.saturating_add(1);
                    reader = BufReader::new(client);
                    continue;
                }
                ForwardOutcome::Done { .. } | ForwardOutcome::MidStreamClosed => {
                    client.shutdown(Shutdown::Both).ok();
                    return;
                }
                ForwardOutcome::BadGateway(msg) => {
                    write_502_body(&mut client, &msg).ok();
                    client.shutdown(Shutdown::Both).ok();
                    return;
                }
            }
        }

        let upstream = match ctx.transport.open_stream(&target, &io_abort) {
            Ok(s) => s,
            Err(msg) => {
                write_502_body(reader.get_mut(), &msg).ok();
                return;
            }
        };
        if !ctx.admits() {
            upstream.close_or_reset();
            return;
        }
        if is_connect {
            if reader.get_mut().write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n").is_err() {
                upstream.close_or_reset();
                return;
            }
        } else {
            let head = build_forwarded_head(&req, &url, true, 0, false);
            if upstream.write_all(head.as_bytes(), &io_abort).is_err() {
                write_status(reader.get_mut(), "502 Bad Gateway").ok();
                upstream.close_or_reset();
                return;
            }
        }

        let mut client_read = reader.into_inner();
        client_read.set_read_timeout(Some(CLIENT_POLL_SLICE)).ok();
        let Ok(mut client_write) = client_read.try_clone() else {
            upstream.close_or_reset();
            return;
        };
        let read_abort = AbortHandle::new();
        if !guard.track_abort(&read_abort) {
            upstream.close_or_reset();
            return;
        }
        if is_ws {
            match await_ws_upgrade(&upstream, &read_abort, &mut client_write) {
                WsHandshake::Upgraded => {}
                WsHandshake::RejectedNot101 => {
                    let _ = upstream.finish_write(&io_abort);
                    drain_remote_to_client(&ctx, client_id, &upstream, &read_abort, &mut client_write);
                    client_read.shutdown(Shutdown::Both).ok();
                    upstream.close_or_reset();
                    return;
                }
                WsHandshake::Closed => {
                    client_read.shutdown(Shutdown::Both).ok();
                    upstream.close_or_reset();
                    return;
                }
            }
        }

        let ctx_down = ctx.clone();
        let up_down = upstream.clone();
        let abort_down = read_abort.clone();
        let down_thread = std::thread::Builder::new()
            .name("browser-tunnel-down".into())
            .spawn(move || {
                drain_remote_to_client(&ctx_down, client_id, &up_down, &abort_down, &mut client_write);
            })
            .ok();

        let clean_eof = pump_bidirectional_stream(
            &ctx,
            client_id,
            &upstream,
            &[],
            &mut client_read,
            &io_abort,
        );
        if clean_eof {
            if let Some(t) = down_thread {
                t.join().ok();
            }
            client_read.shutdown(Shutdown::Both).ok();
            upstream.close_or_reset();
            return;
        }
        read_abort.abort();
        client_read.shutdown(Shutdown::Both).ok();
        upstream.close_or_reset();
        if let Some(t) = down_thread {
            t.join().ok();
        }
        break;
    }
}
