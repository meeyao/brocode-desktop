//! What the UI says about a failure: what happened, why, what to do next, and the details to
//! copy (the call, the server, its status and its own words), as one model every surface shows
//! (`status_toast::error`, the Files panel's failed read). Built from the client's typed error
//! (`opencode_kit::wire_error::WireError`), so OpenCode's own explanation is kept and a captive
//! portal is named for what it is.
//!
//! Wording: plain English, short, no blame; where Desktop's title is right it stays (the caller
//! passes it as `what`).

use opencode_kit::wire_error::{WireError, WireKind};

#[derive(Clone, Debug, PartialEq)]
pub struct UserError {
    /// The headline: what didn't happen ("Couldn't rename session").
    pub what: String,
    /// Why, in the app's words, naming the server.
    pub why: String,
    /// The server's own explanation, shown as its words and apart from ours.
    pub quote: Option<String>,
    /// What to do now, when the button isn't it.
    pub next: Option<String>,
    /// What "Copy details" puts on the clipboard.
    pub details: String,
}

const RETRY_LOGS: &str = "Try again. If it keeps failing, check the server's logs.";
const SIGN_IN_PAGE: &str = "If this network has a sign-in page, sign in, then try again.";

impl UserError {
    /// `e`, from a call to `server` (its name as the app shows it), headlined `what`.
    pub fn from_wire(what: &str, server: &str, e: &WireError) -> UserError {
        let theirs = |m: &Option<String>| m.as_deref().map(str::trim).filter(|m| !m.is_empty()).map(str::to_owned);
        let (why, quote, next): (String, Option<String>, Option<&str>) = match &e.kind {
            WireKind::TooLarge { limit, length } => {
                let size = length.map(opencode_i18n::bytes).unwrap_or_else(|| format!("> {}", opencode_i18n::bytes(*limit)));
                (opencode_i18n::t!("rune.scale.tooLarge", size = size), None, None)
            }
            WireKind::PartialOutput { at_least, .. } => {
                let size = format!("≥ {} B", opencode_i18n::integer(*at_least as i64));
                (opencode_i18n::t!("rune.scale.tooLarge", size = size), None, None)
            }
            WireKind::Unreachable(_) => (format!("{server} isn't answering."), None, Some("Check that it's running and reachable, then try again.")),
            WireKind::TimedOut => (format!("{server} didn't answer in time."), None, Some("It may still be working on it. Try again in a moment.")),
            WireKind::Dropped(_) => (format!("The connection to {server} dropped before it answered."), None, Some("It may have gone through. Check before you try again.")),
            WireKind::Tls(detail) => (format!("The secure connection to {server} failed ({detail})."), None, Some("Check the server's certificate.")),
            WireKind::Redirected { location, .. } => (
                match location {
                    Some(to) => format!("Something on the network answered instead of {server}, and sent the app to {to}."),
                    None => format!("Something on the network answered instead of {server}."),
                },
                None,
                Some(SIGN_IN_PAGE),
            ),
            WireKind::NotOpenCode { what, .. } => (format!("Something on the network answered instead of {server}: {what}."), None, Some(SIGN_IN_PAGE)),
            WireKind::Status { status: 401, .. } => (format!("{server} asks this app to sign in again."), None, Some("Sign in to the server, then try again.")),
            WireKind::Status { status: 403, message, .. } => match theirs(message) {
                Some(m) => (format!("{server} doesn't allow this. It says:"), Some(m), None),
                None => (format!("{server} doesn't allow this."), None, None),
            },
            WireKind::Status { tag: Some(tag), message, .. } if tag == "SkillLoadError" => match theirs(message) {
                Some(m) => (format!("{server} couldn't load a skill. It says:"), Some(m), None),
                None => (format!("{server} couldn't load a skill."), None, None),
            },
            WireKind::Status { status, message, .. } if *status >= 500 => match theirs(message) {
                Some(m) => (format!("{server} hit an error. It says:"), Some(m), Some(RETRY_LOGS)),
                None => (format!("{server} hit an error (HTTP {status})."), None, Some(RETRY_LOGS)),
            },
            WireKind::Status { status, message, .. } => match theirs(message) {
                Some(m) => (format!("{server} refused it. It says:"), Some(m), None),
                None => (format!("{server} refused it (HTTP {status})."), None, None),
            },
            WireKind::Shape(m) | WireKind::Other(m) => (sentence(m), None, None),
        };
        let details = format!("Action: {what}\nServer: {server}\nCall: {} {}\nError: {}", e.method, e.path, e.kind);
        UserError { what: what.to_owned(), why, quote, next: next.map(str::to_owned), details }
    }

    /// Why, the server's words in quotation marks and what to do, one paragraph each, for a
    /// surface that shows one text (the Files panel's failed read).
    pub fn description(&self) -> String {
        let quote = self.quote.as_ref().map(|q| format!("\u{201c}{q}\u{201d}"));
        [Some(self.why.clone()), quote, self.next.clone()].into_iter().flatten().collect::<Vec<_>>().join("\n")
    }
}

/// `m` as a sentence: capitalised, ending with a stop.
fn sentence(m: &str) -> String {
    let m = m.trim();
    let mut out: String = m.chars().take(1).flat_map(char::to_uppercase).chain(m.chars().skip(1)).collect();
    if !out.ends_with(['.', '!', '?']) {
        out.push('.');
    }
    out
}

/// What a surface with one line of its own says about `e` (a row's status, a dialog's note):
/// why, the server's words and what next, one after another, built from the typed error.
pub fn said(s: &crate::App, what: &str, e: &WireError) -> String {
    let server = crate::servers::runtime::label(crate::servers::runtime::active(s));
    UserError::from_wire(what, &server, e).description().replace('\n', " ")
}
