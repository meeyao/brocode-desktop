//! Inline code that names a file becomes a link to it, as Desktop's Markdown marks it
//! (`session-ui/src/components/markdown.tsx` `markInlineCode` and `inlineCodeKind`): code that
//! reads as a path is checked, and shows as a link (`text-code-path`, no chip) while the answer is
//! yes. In a previewed Markdown file the path resolves against the file's folder and is checked
//! by listing that folder (`artifact-view.tsx`'s `links.exists`). In a finished reply it is
//! checked as Desktop's file plugin checks a link with no base (`resolve-link.ts`
//! `checkFileLinkExists`): an absolute path by listing its folder, any other by the server's file
//! search, where the path itself or a unique or several files ending with it count; a click
//! searches again and opens the file, or the Open file picker on the path when several match.

use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::time::Duration;

use opencode_kit::client::Server;
use rune::Tx;

use super::links::{Link, classify, link_directory, open_link};
use crate::{FilesHost, FilesJob, ResolvedCodeLink};

/// What is known about one path (`LinkExistsEntry`, without its expiry: a document's checks go
/// with it).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PathCheck {
    Checking,
    Settled { exists: bool },
}

/// One document's checks, by the code's text.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CodePaths {
    checks: HashMap<String, PathCheck>,
    /// Moves when a check settles, so the preview draws again.
    pub rev: u64,
}

impl CodePaths {
    /// The code naming files that exist.
    pub fn found(&self) -> impl Iterator<Item = &str> {
        self.checks
            .iter()
            .filter(|(_, c)| **c == PathCheck::Settled { exists: true })
            .map(|(k, _)| k.as_str())
    }
}

/// How long an answer about a reply's path stands (`link-exists-machine.ts`): a yes 60 s, a no 5 s.
const FOUND_FOR: Duration = Duration::from_secs(60);
const MISSING_FOR: Duration = Duration::from_secs(5);

mod replies;
pub(crate) use replies::Replies;

/// Inline code that reads as a path (`inlineCodeKind` `path`): no spaces, URL scheme, code or
/// glob characters, nor a network share; a folder separator or a dot beside a letter, or a common
/// file name that has none (`Makefile`).
pub fn reads_as_path(text: &str) -> bool {
    const NAMES: &[&str] = &[
        "CHANGELOG",
        "CODEOWNERS",
        "Containerfile",
        "Dockerfile",
        "Gemfile",
        "Justfile",
        "LICENSE",
        "Makefile",
        "Procfile",
        "README",
        "Rakefile",
        "Vagrantfile",
    ];
    if text.is_empty() || text.contains(char::is_whitespace) || has_scheme(text) {
        return false;
    }
    if text.contains(['{', '}', '<', '>', '"', '\'', '`', ';', '=', '|', '&', '*', ',']) {
        return false;
    }
    // A network share (`\\host\share`), device path, or remote `file:` URI is never probed.
    if browser_policy::is_raw_unc_href(text) {
        return false;
    }
    let path = path_line_suffix(text).0;
    NAMES.contains(&path) || (path.contains(['.', '/', '\\']) && path.contains(|c: char| c.is_ascii_alphabetic()))
}

/// `scheme://` at the start (`/^[a-z][a-z0-9+.-]*:\/\//i`).
fn has_scheme(text: &str) -> bool {
    let Some((head, _)) = text.split_once("://") else { return false };
    let mut chars = head.chars();
    chars.next().is_some_and(|c| c.is_ascii_alphabetic())
        && chars.all(|c| c.is_ascii_alphanumeric() || "+.-".contains(c))
}

/// `parsePathLineSuffix`: `path#L12`, `path#L12-L20` (columns `C5` dropped), `path:12`,
/// `path:12:5`, `path:12-20`: the path, and the lines when both are 1 to 1,000,000. A drive's
/// colon (`C:`) is no line.
pub fn path_line_suffix(input: &str) -> (&str, Option<(u32, u32)>) {
    for (i, _) in input.match_indices('#') {
        if let Some(lines) = hash_lines(&input[i + 1..]) {
            return (&input[..i], lines);
        }
    }
    for (i, _) in input.match_indices(':') {
        let path = &input[..i];
        let drive = path.len() <= 2
            && path.chars().next().is_some_and(|c| c.is_ascii_alphabetic())
            && path[1..].chars().all(|c| c == ':');
        if !drive && let Some(lines) = colon_lines(&input[i + 1..]) {
            return (path, lines);
        }
    }
    (input, None)
}

/// `L12`, `L12C5`, `L12-L20`, `L12-20C3` (`lineHashSuffixPattern`'s tail).
fn hash_lines(s: &str) -> Option<Option<(u32, u32)>> {
    let s = s.strip_prefix(['L', 'l'])?;
    let (a, rest) = digits(s)?;
    let rest = column(rest, ['C', 'c']);
    if rest.is_empty() {
        return Some(range(a, a));
    }
    let rest = rest.strip_prefix('-')?;
    let rest = rest.strip_prefix(['L', 'l']).unwrap_or(rest);
    let (b, rest) = digits(rest)?;
    column(rest, ['C', 'c']).is_empty().then(|| range(a, b))
}

/// `12`, `12:5`, `12-20`, `12:5-20:3`, each with a trailing `:` allowed (`lineColonSuffixPattern`'s tail).
fn colon_lines(s: &str) -> Option<Option<(u32, u32)>> {
    let (a, rest) = digits(s)?;
    let rest = column(rest, [':', ':']);
    let (b, rest) = match rest.strip_prefix('-') {
        Some(r) => {
            let (b, r) = digits(r)?;
            (b, column(r, [':', ':']))
        }
        None => (a, rest),
    };
    matches!(rest, "" | ":").then(|| range(a, b))
}

fn digits(s: &str) -> Option<(u64, &str)> {
    let n = s.bytes().take_while(u8::is_ascii_digit).count();
    (n > 0).then(|| (s[..n].parse::<u64>().unwrap_or(u64::MAX), &s[n..]))
}

/// `s` past a column (`C5` or `:5`) when one leads it.
fn column(s: &str, mark: [char; 2]) -> &str {
    match s.strip_prefix(mark).and_then(digits) {
        Some((_, rest)) => rest,
        None => s,
    }
}

fn range(a: u64, b: u64) -> Option<(u32, u32)> {
    let ok = |n: u64| (1..=1_000_000).contains(&n);
    (ok(a) && ok(b)).then(|| (a.min(b) as u32, a.max(b) as u32))
}

/// `parseFileLink` for inline code: the path with `/` separators, `./` and `..` collapsed
/// (a relative path keeps its leading `..`), and its lines.
fn file_link(code: &str) -> (String, Option<(u32, u32)>) {
    let raw = code.trim().replace('\\', "/");
    let (path, lines) = path_line_suffix(&raw);
    let path = path.strip_prefix("./").unwrap_or(path).trim_end_matches('/');
    let drive = path.len() >= 3
        && path.as_bytes()[1] == b':'
        && path.as_bytes()[2] == b'/'
        && path.as_bytes()[0].is_ascii_alphabetic();
    let (prefix, rest) = match () {
        _ if drive => (&path[..3], &path[3..]),
        _ if path.starts_with("//") => ("//", &path[2..]),
        _ if path.starts_with('/') => ("/", &path[1..]),
        _ => ("", path),
    };
    let mut out: Vec<&str> = Vec::new();
    for part in rest.split('/') {
        match part {
            "" | "." => {}
            ".." if out.last().is_some_and(|l| *l != "..") => {
                out.pop();
            }
            ".." if prefix.is_empty() => out.push(".."),
            ".." => {}
            p => out.push(p),
        }
    }
    (format!("{prefix}{}", out.join("/")), lines)
}

fn is_absolute_link(path: &str) -> bool {
    path.starts_with('/')
        || (path.len() >= 3 && path.as_bytes()[0].is_ascii_alphabetic() && &path[1..3] == ":/")
}

/// What a relative path names in the workspace (`findFileLink`): the path itself when the search
/// finds it, else the one file ending with it, else the picker on it for several.
pub(crate) fn execute_resolve_code_link(srv: &Server, dir: &str, path: &str) -> Option<ResolvedCodeLink> {
    let mut query = path;
    while let Some(rest) = query.strip_prefix("../") {
        query = rest;
    }
    if query.is_empty() || query == ".." {
        return None;
    }
    let tail = format!("/{query}");
    let matches: Vec<String> = super::data::fetch_live_find(srv, dir, query, 200)
        .into_iter()
        .filter(|f| f == query || f.ends_with(&tail))
        .collect();
    if matches.iter().any(|f| f == query) {
        return Some(ResolvedCodeLink::File(query.to_owned()));
    }
    match matches.len() {
        0 => None,
        1 => matches.into_iter().next().map(ResolvedCodeLink::File),
        _ => Some(ResolvedCodeLink::Picker(query.to_owned())),
    }
}

/// Checks whether workspace path `target` exists inside `dir` by listing its parent folder.
pub(crate) fn execute_check_code_path(srv: &Server, dir: &str, target: &str) -> bool {
    let parent = opencode_kit::path::parent(target);
    super::data::fetch_live_dir(srv, dir, parent)
        .iter()
        .any(|(p, is_dir)| !is_dir && p == target)
}

/// `checkFileLinkExists` with no base: `~` names the server's `home` (no until it is known,
/// opencode#65689) and network shares are no; an absolute path is listed in its folder; any other is
/// searched for.
fn reply_path_exists(srv: &Server, dir: &str, home: &str, code: &str) -> bool {
    if browser_policy::is_raw_unc_href(code) {
        return false;
    }
    let (path, _) = file_link(code);
    let home = home.replace('\\', "/");
    let expanded = path.strip_prefix("~/").or((path == "~").then_some("")).filter(|_| !home.is_empty()).map(|rest| {
        let base = home.trim_end_matches('/');
        if rest.is_empty() { base.to_owned() } else { format!("{base}/{rest}") }
    });
    let path = expanded.unwrap_or(path);
    if path.is_empty() || path.starts_with('~') || path.starts_with("//") || browser_policy::is_raw_unc_href(&path) {
        return false;
    }
    if !is_absolute_link(&path) {
        return execute_resolve_code_link(srv, dir, &path).is_some();
    }
    // On a local server, verify fixed-drive/MNT_LOCAL and zero reparse points BEFORE asking the
    // server to list an absolute parent directory (prevents zero-click SMB/autofs probes on
    // mapped drives, `subst`-to-UNC, junctions, or `/net/`).
    if srv.is_local() {
        return browser_policy::verify_local_path(&path).is_ok_and(|v| v.is_file);
    }
    // Lists the folder instead of reading the file, so a check moves no content.
    let name = opencode_kit::path::basename(&path);
    super::data::fetch_live_dir(srv, dir, opencode_kit::path::parent(&path))
        .iter()
        .any(|(p, is_dir)| !is_dir && opencode_kit::path::basename(p) == name)
}

/// The single-backtick code spans of Markdown `src` outside fenced blocks that read as paths.
fn candidates(src: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut fenced = false;
    for line in src.lines() {
        if line.trim_start().starts_with("```") || line.trim_start().starts_with("~~~") {
            fenced = !fenced;
            continue;
        }
        if fenced {
            continue;
        }
        let mut parts = line.split('`');
        parts.next();
        while let (Some(code), Some(_)) = (parts.next(), parts.next()) {
            let code = code.trim();
            if reads_as_path(code) && !out.iter().any(|c| c == code) {
                out.push(code.to_owned());
            }
        }
    }
    out
}

/// Checks the paths Markdown file `path`'s inline code names, once each.
pub fn check_code_paths(host: &mut dyn FilesHost, sid: &str, path: &str) {
    if !super::markdown::is_markdown(path) {
        return;
    }
    let todo: Vec<String> = {
        let Some(ws) = host.files().sessions.get(sid) else { return };
        let Some(doc) = ws.docs.get(path).filter(|d| d.content().is_text()) else { return };
        let known = ws.code_paths.get(path);
        candidates(doc.text())
            .into_iter()
            .filter(|c| known.is_none_or(|k| !k.checks.contains_key(c)))
            .collect()
    };
    if todo.is_empty() {
        return;
    }
    let dir = link_directory(host, sid);
    let base = opencode_kit::path::parent(path).to_owned();
    let home = host.home_directory().into_owned();
    let live = host.is_live();
    let mut jobs = Vec::new();
    {
        let ws = host.files_mut().sessions.entry(sid.to_owned()).or_default();
        for code in todo {
            let target = match classify(&code, &base, &dir, &home) {
                Link::Inside { path, .. } => Some(path),
                Link::Web(_) | Link::Heading(_) | Link::Outside { .. } | Link::Skill { .. } => None,
            };
            let known_exists = target.as_ref().is_some_and(|t| ws.known_files.contains(t));
            let checks = ws.code_paths.entry(path.to_owned()).or_default();
            let Some(target) = target else {
                checks.checks.insert(code, PathCheck::Settled { exists: false });
                continue;
            };
            if !live {
                checks.checks.insert(code, PathCheck::Settled { exists: known_exists });
                checks.rev += 1;
                continue;
            }
            checks.checks.insert(code.clone(), PathCheck::Checking);
            jobs.push(FilesJob::CheckCodePath {
                session_id: sid.to_owned(),
                doc_path: path.to_owned(),
                code,
                directory: dir.clone(),
                target,
            });
        }
    }
    for job in jobs {
        host.spawn_job(job);
    }
}

pub(crate) fn apply_code_path_checked(
    host: &mut dyn FilesHost,
    sid: &str,
    doc: &str,
    code: String,
    exists: bool,
    tx: &mut Tx,
) {
    let Some(checks) = host
        .files_mut()
        .sessions
        .get_mut(sid)
        .and_then(|ws| ws.code_paths.get_mut(doc))
    else {
        return;
    };
    if checks.checks.get(&code) == Some(&PathCheck::Checking) {
        checks.checks.insert(code, PathCheck::Settled { exists });
        checks.rev += 1;
        tx.instant();
    }
}

/// Paths known to exist now. Reading new candidates starts their checks, not a second view;
/// neither a streaming block nor a window without a server has anything to check.
pub fn reply_code_links(host: &dyn FilesHost, body: &str, live: bool) -> Option<Rc<HashSet<String>>> {
    if live || !body.contains('`') {
        return None;
    }
    let server = host.live_server()?;
    let sid = host.active_session_id()?;
    let codes = candidates(body);
    if codes.is_empty() {
        return None;
    }
    let waker = host.waker();
    host.files_untracked()
        .replies
        .read(sid, codes, server, || link_directory(host, sid), host.home_directory().into_owned(), waker.as_ref())
}

/// A click on reply code naming a file (`code`): a plugin's link handler takes it first, as
/// any link; else the file opens at its lines, or the Open file picker on the path when several
/// files end with it; a path the search doesn't find still opens as written, which covers
/// files the search skips (an ignored `.env`).
pub fn open_code_path(host: &mut dyn FilesHost, code: &str, tx: &mut Tx) {
    if browser_policy::is_raw_unc_href(code) {
        host.toast_say(opencode_i18n::t!("rune.files.link.outside").to_owned(), code.trim().to_owned());
        return;
    }
    if host.plugin_open_link(code, tx) {
        return;
    }
    let Some(sid) = host.active_session_id().map(str::to_owned) else { return };
    let (path, lines) = file_link(code);
    if path.is_empty() || is_absolute_link(&path) || path.starts_with('~') || path.starts_with("//") {
        return open_link(host, &sid, None, code, tx);
    }
    let dir = link_directory(host, &sid);
    host.spawn_job(FilesJob::ResolveCodeLink {
        session_id: sid,
        directory: dir,
        path,
        code: code.to_owned(),
        lines,
    });
}

pub(crate) fn apply_code_link_resolved(
    host: &mut dyn FilesHost,
    sid: &str,
    code: &str,
    lines: Option<(u32, u32)>,
    target: Option<ResolvedCodeLink>,
    tx: &mut Tx,
) {
    if host.active_session_id() != Some(sid) {
        return;
    }
    match target {
        Some(ResolvedCodeLink::Picker(query)) => open_file_picker(host, &query, tx),
        Some(ResolvedCodeLink::File(file)) => {
            let href = match lines {
                Some((a, b)) => format!("{file}:{a}-{b}"),
                None => file,
            };
            open_link(host, sid, None, &href, tx);
        }
        None => open_link(host, sid, None, code, tx),
    }
}

/// The side region's Open file picker, its filter holding `query` (`renderer.tsx` `openPicker`).
pub fn open_file_picker(host: &mut dyn FilesHost, query: &str, tx: &mut Tx) {
    let Some(sid) = host.active_session_id().map(str::to_owned) else { return };
    crate::ops::ensure_workspace_files(host, &sid);
    {
        let pane = host.files_mut();
        pane.filter = rune::TextEdit::new(query);
        pane.filter.focused = true;
    }
    host.open_side_tab(&sid, super::data::OPEN_TAB_KEY, true, true, false, tx);
    crate::ops::request_sidebar_file_search(host);
}
