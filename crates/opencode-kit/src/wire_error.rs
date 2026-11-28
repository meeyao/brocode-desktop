//! Why a call to a server didn't give what it asked for, as the client saw it: no connection, no
//! answer in time, OpenCode's own refusal (with the explanation it sent), or an answer that
//! didn't come from OpenCode at all (a captive portal's sign-in page, a proxy's error page).

use std::fmt;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WireError {
    pub method: &'static str,
    /// The path asked for, query included.
    pub path: String,
    pub kind: WireKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WireKind {
    /// No connection was made: nothing listening, the host unknown, the network unreachable. The
    /// request never left.
    Unreachable(String),
    /// No answer within the call's time limit; the request may have arrived.
    TimedOut,
    /// The connection broke after the request went (reset, closed early); it may have arrived.
    Dropped(String),
    /// The secure connection couldn't be set up (an untrusted certificate, say).
    Tls(String),
    /// A redirect. OpenCode's API never redirects, so something on the network answered: a
    /// captive portal's sign-in page, a proxy, a wrong address.
    Redirected { status: u16, location: Option<String> },
    /// An answer that isn't OpenCode's: a web page, another type, a body that doesn't parse.
    NotOpenCode { status: u16, what: String },
    /// OpenCode answered with an error status; `message` is its own explanation when it sent one,
    /// `tag` its error's name (`SessionNotFoundError`).
    Status { status: u16, tag: Option<String>, message: Option<String> },
    /// OpenCode's answer, without what the call needed from it.
    Shape(String),
    /// The response exceeded its inline budget. Length is the declared transfer length when
    /// the headers alone established the refusal, not the size of a tool string inside JSON.
    TooLarge { limit: u64, length: Option<u64> },
    /// Decoded prefix of one tool's interrupted text item. Never the full output or its size.
    PartialOutput { head: std::sync::Arc<str>, at_least: u64 },
    /// Anything else the transport reported.
    Other(String),
}

/// The words an error's text carries when the answer wasn't OpenCode's, so a reading of the text
/// never takes it for OpenCode's refusal.
pub const NOT_OPENCODE: &str = "not Brocode's answer";

impl WireError {
    pub fn new(method: &'static str, path: &str, kind: WireKind) -> WireError {
        WireError { method, path: path.to_owned(), kind }
    }

    /// The HTTP status OpenCode answered with, when it refused.
    pub fn status(&self) -> Option<u16> {
        match self.kind {
            WireKind::Status { status, .. } => Some(status),
            _ => None,
        }
    }

    /// The server's own explanation, when it sent one.
    pub fn message(&self) -> Option<&str> {
        match &self.kind {
            WireKind::Status { message: Some(m), .. } if !m.trim().is_empty() => Some(m.trim()),
            _ => None,
        }
    }

    /// Structured `SkillLoadError` returned when preparing a prompt or loading a skill fails
    /// (`packages/protocol/src/groups/session.ts`, `opencode#66207`).
    pub fn is_skill_load_error(&self) -> bool {
        matches!(&self.kind, WireKind::Status { tag: Some(t), .. } if t == "SkillLoadError")
    }

    /// Whether the call certainly never reached OpenCode: no connection was made, or OpenCode
    /// refused it (a 4xx or a pre-admission `SkillLoadError`). A timeout, a dropped connection or
    /// any other 5xx may have arrived.
    pub fn failed_for_sure(&self) -> bool {
        matches!(self.kind, WireKind::Unreachable(_))
            || self.status().is_some_and(|s| (400..500).contains(&s))
            || self.is_skill_load_error()
    }

    /// Whether the same call is worth asking again: the server may be busy or briefly gone. A 4xx,
    /// a structured `SkillLoadError` (HTTP 502), or an answer that isn't OpenCode's won't change
    /// on a retry.
    pub fn transient(&self) -> bool {
        matches!(self.kind, WireKind::Unreachable(_) | WireKind::TimedOut | WireKind::Dropped(_))
            || matches!(&self.kind, WireKind::Status { status: 502..=504, tag, .. } if tag.as_deref() != Some("SkillLoadError"))
    }

    /// Something other than OpenCode answered (`Redirected`, `NotOpenCode`).
    pub fn intercepted(&self) -> bool {
        matches!(self.kind, WireKind::Redirected { .. } | WireKind::NotOpenCode { .. })
    }
}

impl fmt::Display for WireError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.method.is_empty() {
            return write!(f, "{}", self.kind);
        }
        write!(f, "{} {}: {}", self.method, self.path, self.kind)
    }
}

/// A failure of a chain of calls that isn't one call's (a rewrite stopped, a step that never ran):
/// `Other`, with no call of its own.
impl From<&str> for WireError {
    fn from(what: &str) -> WireError {
        WireError { method: "", path: String::new(), kind: WireKind::Other(what.to_owned()) }
    }
}

impl From<String> for WireError {
    fn from(what: String) -> WireError {
        WireError { method: "", path: String::new(), kind: WireKind::Other(what) }
    }
}

impl fmt::Display for WireKind {
    // The text keeps ureq's `http status: <n>` at the end of a refusal and `connection failed` for
    // no connection, which readers of error text (`plain_error`) still look for.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            WireKind::Unreachable(e) => write!(f, "connection failed: {e}"),
            WireKind::TimedOut => write!(f, "timeout"),
            WireKind::TooLarge { limit, length } => write!(f, "body length {length:?} exceeds {limit} bytes"),
            WireKind::PartialOutput { at_least, .. } => write!(f, "partial tool output, at least {at_least} bytes"),
            WireKind::Dropped(e) => write!(f, "connection dropped: {e}"),
            WireKind::Tls(e) => write!(f, "tls: {e}"),
            WireKind::Redirected { status, location } => {
                let to = location.as_deref().map_or("another page", |l| l.split('?').next().unwrap_or(l));
                write!(f, "{NOT_OPENCODE}: a redirect ({status}) to {to}")
            }
            WireKind::NotOpenCode { status, what } => write!(f, "{NOT_OPENCODE}: {what}, status {status}"),
            WireKind::Status { status, message, .. } => match message {
                Some(m) => write!(f, "{m}: http status: {status}"),
                None => write!(f, "http status: {status}"),
            },
            WireKind::Shape(e) | WireKind::Other(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for WireError {}

impl WireError {
    /// The error back from its text ([`Display`](fmt::Display)): the app's channels carry a
    /// failed call as text (`Incoming::Status`, `SendFailed`), so the UI reads it back into what
    /// it was to say it in words. `None` for text no call wrote.
    pub fn parse(text: &str) -> Option<WireError> {
        let (call, rest) = text.split_once(": ")?;
        let (method, path) = call.split_once(' ')?;
        let method = ["GET", "POST", "PATCH", "PUT", "DELETE"].into_iter().find(|m| *m == method)?;
        let kind = if let Some(e) = rest.strip_prefix("connection failed: ") {
            WireKind::Unreachable(e.to_owned())
        } else if rest == "timeout" {
            WireKind::TimedOut
        } else if let Some(e) = rest.strip_prefix("connection dropped: ") {
            WireKind::Dropped(e.to_owned())
        } else if let Some(e) = rest.strip_prefix("tls: ") {
            WireKind::Tls(e.to_owned())
        } else if let Some(what) = rest.strip_prefix(NOT_OPENCODE).and_then(|r| r.strip_prefix(": ")) {
            match what.strip_prefix("a redirect (") {
                Some(r) => {
                    let (status, to) = r.split_once(") to ").unwrap_or((r, ""));
                    WireKind::Redirected { status: status.parse().unwrap_or(302), location: (!to.is_empty() && to != "another page").then(|| to.to_owned()) }
                }
                None => {
                    let (what, status) = what.rsplit_once(", status ").unwrap_or((what, "200"));
                    WireKind::NotOpenCode { status: status.parse().unwrap_or(200), what: what.to_owned() }
                }
            }
        } else if let Some((message, status)) = rest.rsplit_once("http status: ") {
            let message = message.trim_end_matches(": ").trim();
            WireKind::Status { status: status.trim().parse().ok()?, tag: None, message: (!message.is_empty()).then(|| message.to_owned()) }
        } else {
            WireKind::Other(rest.to_owned())
        };
        Some(WireError { method, path: path.to_owned(), kind })
    }
}

impl From<WireError> for String {
    fn from(e: WireError) -> String {
        e.to_string()
    }
}

/// A transport failure in our words.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn transport(e: ureq::Error) -> WireKind {
    use std::io::ErrorKind;
    match e {
        ureq::Error::Timeout(_) => WireKind::TimedOut,
        ureq::Error::BodyExceedsLimit(limit) => WireKind::TooLarge { limit, length: None },
        ureq::Error::HostNotFound => WireKind::Unreachable("host not found".into()),
        ureq::Error::ConnectionFailed => WireKind::Unreachable("no connection".into()),
        ureq::Error::Io(io) => match io.kind() {
            ErrorKind::ConnectionRefused | ErrorKind::AddrNotAvailable | ErrorKind::NetworkUnreachable | ErrorKind::HostUnreachable | ErrorKind::NetworkDown => {
                WireKind::Unreachable(io.to_string())
            }
            ErrorKind::TimedOut | ErrorKind::WouldBlock => WireKind::TimedOut,
            ErrorKind::ConnectionReset | ErrorKind::ConnectionAborted | ErrorKind::UnexpectedEof | ErrorKind::BrokenPipe => WireKind::Dropped(io.to_string()),
            _ => WireKind::Other(io.to_string()),
        },
        ureq::Error::Tls(e) => WireKind::Tls(e.to_owned()),
        // rustls's errors exist only in builds with that feature; their words say what they are.
        e if e.to_string().to_ascii_lowercase().contains("certificate") => WireKind::Tls(e.to_string()),
        e => WireKind::Other(e.to_string()),
    }
}

/// A read of a body already coming that failed: the connection broke or went silent past the
/// read's limit.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn read_failure(e: std::io::Error) -> WireKind {
    use std::io::ErrorKind;
    match e.kind() {
        ErrorKind::TimedOut | ErrorKind::WouldBlock => WireKind::TimedOut,
        ErrorKind::ConnectionReset | ErrorKind::ConnectionAborted | ErrorKind::UnexpectedEof | ErrorKind::BrokenPipe => WireKind::Dropped(e.to_string()),
        _ => {
            let text = e.to_string();
            // ureq hands its own errors (its read timeout) through as the io error's source.
            match e.into_inner().and_then(|b| b.downcast::<ureq::Error>().ok()) {
                Some(u) => transport(*u),
                None => WireKind::Dropped(text),
            }
        }
    }
}

/// A browser says only that it couldn't make the request (no server, or CORS refused).
#[cfg(target_arch = "wasm32")]
pub(crate) fn transport(e: ureq::Error) -> WireKind {
    match e {
        ureq::Error::Timeout(_) => WireKind::TimedOut,
        ureq::Error::StatusCode(status) => WireKind::Status { status, tag: None, message: None },
        ureq::Error::Io(e) if e.contains("connection failed") => WireKind::Unreachable(e),
        ureq::Error::Io(e) => WireKind::Other(e),
    }
}

/// OpenCode's explanation in an error's body: `{_tag, message}` (its errors), or the shapes older
/// servers and proxies in front of them use (`{error: {message}}`, `{data: {message}}`,
/// `{message}`); a short plain-text body as it is.
pub(crate) fn explanation(content_type: &str, body: &[u8]) -> (Option<String>, Option<String>) {
    let text = String::from_utf8_lossy(&body[..body.len().min(64 << 10)]);
    if is_json(content_type) {
        let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) else { return (None, None) };
        let tag = v["_tag"].as_str().or_else(|| v["name"].as_str()).or_else(|| v["error"]["code"].as_str()).map(str::to_owned);
        let mut message = [&v["message"], &v["error"]["message"], &v["data"]["message"], &v["error"]]
            .into_iter()
            .find_map(|m| m.as_str().map(str::trim).filter(|m| !m.is_empty()))
            .map(|m| m.lines().next().unwrap_or(m).to_owned());
        // Include the failing skill identifier for structured SkillLoadError responses (opencode#66207).
        if tag.as_deref() == Some("SkillLoadError")
            && let Some(skill) = v["skill"].as_str().map(str::trim).filter(|s| !s.is_empty())
        {
            message = Some(match message {
                Some(m) if m.contains(skill) => m,
                Some(m) => format!("Skill \"{skill}\": {m}"),
                None => format!("Failed to load skill \"{skill}\""),
            });
        }
        return (tag, message);
    }
    let plain = content_type.split(';').next().is_some_and(|t| t.trim().eq_ignore_ascii_case("text/plain"));
    let line = text.trim();
    (None, (plain && !line.is_empty() && line.len() <= 300 && !line.contains('\n')).then(|| line.to_owned()))
}

pub(crate) fn is_json(content_type: &str) -> bool {
    let t = content_type.split(';').next().unwrap_or("").trim();
    t.eq_ignore_ascii_case("application/json") || t.to_ascii_lowercase().ends_with("+json")
}
