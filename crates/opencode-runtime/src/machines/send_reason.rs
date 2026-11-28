//! Only describe what the failed request proved. A failed response can follow a successful admission.

use opencode_kit::wire_error::{WireError, WireKind};

/// What an error says when it proves nothing about the prompt (a read that didn't parse, say).
pub const NO_RECEIPT: &str = "no delivery receipt";

pub fn words(error: &WireError) -> &'static str {
    // Something between the app and OpenCode answered (`opencode_kit::wire_error`): a captive
    // portal or a proxy. It isn't a refusal by OpenCode, so it never reads as one.
    match &error.kind {
        WireKind::Redirected { .. } => return "the network sent it to a sign-in page",
        WireKind::NotOpenCode { what, .. } if what.contains("text/html") => return "the network answered with a web page, not Brocode",
        WireKind::NotOpenCode { .. } => return "the answer didn't come from Brocode",
        _ => {}
    }
    if error.is_skill_load_error() {
        return "couldn't load a skill";
    }
    match error.status() {
        Some(401) => "sign in to Brocode",
        Some(403) => "permission denied",
        Some(404) => "session not found",
        Some(408 | 504) => "server timed out",
        Some(409) => "the session rejected this message",
        Some(413) => "message is too large",
        Some(425 | 429 | 503) => "server is busy",
        Some(500..=599) => "server error",
        Some(400..=499) => "Brocode rejected this message",
        _ => match &error.kind {
            WireKind::TimedOut => "server didn't answer in time",
            WireKind::Unreachable(_) => "can't reach Brocode",
            WireKind::Dropped(_) => "connection interrupted",
            _ => NO_RECEIPT,
        },
    }
}
