//! A transport error's plain words, for what the UI says. The raw text (`GET /x: timeout:
//! global`) is for logs.

/// Whether the server may simply be busy or briefly gone, so the same read is worth asking again.
/// A 4xx or an unreadable reply won't change on a retry.
pub fn is_transient(raw: &str) -> bool {
    let e = raw.to_lowercase();
    ["timeout", "timed out", "connection", "refused", "broken pipe", "unexpected eof", "stream ended", "http status: 502", "http status: 503", "http status: 504"]
        .iter()
        .any(|k| e.contains(k))
}

/// Whether a failed write certainly never reached the server: a refusal (4xx) or no connection.
/// A timeout, a dropped connection or a 5xx may have arrived, so those are not.
pub fn failed_for_sure(raw: &str) -> bool {
    let e = raw.to_lowercase();
    let client_error = e.find("http status: 4").is_some();
    client_error || ["refused", "connection failed", "failed to lookup", "dns", "no route to host", "network is unreachable"].iter().any(|k| e.contains(k))
}

/// `raw` (with or without its `GET /path: ` prefix) in words a person can read; an error with
/// no known mapping comes back unchanged.
pub fn plain_error(raw: &str) -> String {
    let e = raw.to_lowercase();
    let has = |ks: &[&str]| ks.iter().any(|k| e.contains(k));
    let words = if has(&["timeout", "timed out"]) {
        "Brocode didn't answer in time"
    } else if has(&["refused", "connection failed"]) {
        "Brocode isn't running"
    } else if has(&["connection reset", "connection aborted", "broken pipe", "unexpected eof", "stream ended"]) {
        "Brocode dropped the connection"
    } else if has(&["failed to lookup", "dns", "no route to host", "network is unreachable"]) {
        "Can't reach the Brocode server"
    } else {
        return raw.to_owned();
    };
    words.to_owned()
}

