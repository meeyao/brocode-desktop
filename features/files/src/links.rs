//! Links in Markdown (replies, and files rendered in the viewer): web and app links open as
//! before; a file link opens the file in the side file viewer, as Desktop's file `LinkHandler`
//! does (`gui-plugins/src/file/renderer.tsx`, `resolve-link.ts`).
//!
//! | Link | Opens |
//! | --- | --- |
//! | `https:`, `mailto:`, `brocode:`, any scheme but `file:` | The browser or the app that owns it (live runs only) |
//! | `docs/plan.md`, `./x.md`, `../x.md`, `/abs/in/folder`, `~/…`, `file://…` | The file's tab, at `#L12`, `#L12-L20`, `:12` or `:12-20`; a `#heading` scrolls a rendered file to it |
//! | An HTML file in the session's folder, on the app's own server with the browser on and the window wide | A page in the browser pane (`fileOrPage`, [`FilesHost::opens_as_page`]) |
//! | A file outside the session's folder | Revealed in the file manager on a local fixed drive (never executed; UNC/device paths refused before touching disk) |
//! | `skill://<id>/<file>`, a skill's virtual file | The file's tab, read through the server, at `:12` or `:12-20` |
//! | A file that isn't there | A toast naming it |
//! | A bare word (`link`, `Makefile`) that isn't a file there | A toast saying it points nowhere (a bare word that is a file opens it) |
//!
//! Relative links resolve against the session's folder in a reply, and against the file's own
//! folder in a rendered file.

use opencode_kit::path::is_absolute;
use rune::Tx;

use crate::{FilesHost, LinkOpen, code_paths, data, markdown, ops};

/// Where a link goes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Link {
    /// A web or app address, opened as it is.
    Web(String),
    /// A heading in the document the link is in (`#usage`).
    Heading(String),
    /// A file in the session's folder, relative to it.
    Inside { path: String, lines: Option<(u32, u32)>, heading: Option<String> },
    /// A file outside the session's folder, absolute (or as written, when there's no folder).
    Outside { path: String },
    /// A skill's virtual file, `skill://<id>/<file>` (`data::is_virtual_skill`): read through
    /// the server and shown in the viewer as any file (official
    /// `gui-plugins/src/file/resolve-link.ts:27-29`, `:109-120`).
    Skill { path: String, lines: Option<(u32, u32)> },
}

/// Classifies `href` from a document in folder `base` (relative to `directory`, the session's
/// folder; `""` for a reply). `home` expands `~`.
pub fn classify(href: &str, base: &str, directory: &str, home: &str) -> Link {
    let href = href.trim();
    if let Some(frag) = href.strip_prefix('#') {
        return Link::Heading(decode(frag));
    }
    if href.get(..8).is_some_and(|s| s.eq_ignore_ascii_case("skill://")) {
        let raw = href.split_once('#').map_or(href, |(path, _)| path);
        let (raw, lines) = split_line_suffix(raw);
        return Link::Skill { path: format!("skill://{}", collapse(&decode(raw.get(8..).unwrap_or_default()))), lines };
    }
    let file_url = href.get(..7).is_some_and(|s| s.eq_ignore_ascii_case("file://"));
    // A relative link in a skill's own file names another file of that skill. Official joins the
    // two into `skill:/<id>/...`, one slash (`gui-plugins/src/file/artifact.ts:123-143` from
    // `renderer.tsx:708`), which reads as no skill's file: not copied.
    if let Some(skill) = base.get(..8).filter(|s| s.eq_ignore_ascii_case("skill://")).map(|_| &base[8..])
        && !file_url
        && !scheme(href)
        && !is_absolute(href)
        && !href.starts_with('~')
    {
        let raw = href.split_once('#').map_or(href, |(path, _)| path);
        let (raw, lines) = split_line_suffix(raw);
        return Link::Skill { path: format!("skill://{}", collapse(&format!("{skill}/{}", decode(raw)))), lines };
    }
    if !file_url && scheme(href) {
        return Link::Web(href.to_owned());
    }
    let raw = if file_url {
        let rest = &href[7..];
        let rest = rest.strip_prefix("localhost").unwrap_or(rest);
        rest.split('?').next().unwrap_or(rest)
    } else {
        href
    };
    let (raw, frag) = match raw.split_once('#') {
        Some((p, f)) => (p, Some(f)),
        None => (raw, None),
    };
    let (raw, mut lines) = split_line_suffix(raw);
    let mut heading = None;
    if let Some(f) = frag.filter(|f| !f.is_empty()) {
        match parse_l_fragment(f) {
            Some(l) => lines = Some(l),
            None => heading = Some(decode(f)),
        }
    }
    let path = decode(raw).replace('\\', "/");
    // `file:///C:/notes.md` names `C:/notes.md`.
    let path = match path.strip_prefix('/') {
        Some(p) if p.as_bytes().get(1) == Some(&b':') && is_absolute(p) => p.to_owned(),
        _ => path,
    };
    let (directory, home) = (directory.replace('\\', "/"), home.replace('\\', "/"));
    let dir = directory.trim_end_matches('/');
    let joined = if let Some(rest) = path.strip_prefix("~/").or((path == "~").then_some("")) {
        format!("{}/{rest}", home.trim_end_matches('/'))
    } else if is_absolute(&path) {
        path
    } else {
        let base = base.trim_matches('/');
        let rel = if base.is_empty() { path } else { format!("{base}/{path}") };
        if dir.is_empty() { rel } else { format!("{dir}/{rel}") }
    };
    let abs = is_absolute(&joined);
    let norm = collapse(&joined);
    if !dir.is_empty() {
        return match data::relative_path(&norm, dir) {
            Some(path) => Link::Inside { path, lines, heading },
            None => Link::Outside { path: norm },
        };
    }
    if abs || norm.starts_with("../") || norm == ".." {
        return Link::Outside { path: norm };
    }
    Link::Inside { path: norm, lines, heading }
}

/// Whether `path` is one bare word (`link`, `Makefile`): no folder, no extension.
pub(crate) fn is_bare_word(path: &str) -> bool {
    !path.is_empty()
        && !path.contains(['/', '.', ':', '\\', '%', '#', '~'])
        && !path.contains(char::is_whitespace)
}

/// Whether `href` starts with a URL scheme (`https:`, `mailto:`, `brocode:`) rather than a path
/// with a line (`README.md:12`).
fn scheme(href: &str) -> bool {
    let Some((head, rest)) = href.split_once(':') else { return false };
    let mut chars = head.chars();
    let named = chars.next().is_some_and(|c| c.is_ascii_alphabetic())
        && chars.all(|c| c.is_ascii_alphanumeric() || "+.-".contains(c));
    named && head.len() > 1 && !rest.starts_with(|c: char| c.is_ascii_digit())
}

/// `path:12`, `path:12-20` and `path:12:5` (a column, dropped): the path and its lines.
fn split_line_suffix(raw: &str) -> (&str, Option<(u32, u32)>) {
    let mut parts = raw.rsplitn(3, ':');
    let (Some(last), Some(mid)) = (parts.next(), parts.next()) else { return (raw, None) };
    // `path:12:5`: line 12, column 5.
    if let Some(first) = parts.next()
        && !first.is_empty()
        && let (Ok(line), true) = (mid.parse::<u32>(), last.chars().all(|c| c.is_ascii_digit()) && !last.is_empty())
    {
        return (first, Some((line.max(1), line.max(1))));
    }
    let base = &raw[..raw.len() - last.len() - 1];
    match parse_range(last) {
        Some(r) if !base.is_empty() => (base, Some(r)),
        _ => (raw, None),
    }
}

/// `L12`, `L12-L20`, `L12-20`.
fn parse_l_fragment(f: &str) -> Option<(u32, u32)> {
    let rest = f.strip_prefix('L')?;
    let (a, b) = match rest.split_once('-') {
        Some((a, b)) => (a, Some(b.strip_prefix('L').unwrap_or(b))),
        None => (rest, None),
    };
    let a = a.parse::<u32>().ok()?.max(1);
    let b = match b {
        Some(b) => b.parse::<u32>().ok()?.max(a),
        None => a,
    };
    Some((a, b))
}

/// `12` or `12-20`.
fn parse_range(s: &str) -> Option<(u32, u32)> {
    let (a, b) = s.split_once('-').unwrap_or((s, s));
    let a = a.parse::<u32>().ok()?.max(1);
    Some((a, b.parse::<u32>().ok()?.max(a)))
}

/// `a/./b/../c` → `a/c`; leading `..` stay on a relative path and drop at `/`.
fn collapse(path: &str) -> String {
    let abs = is_absolute(path);
    let mut out: Vec<&str> = Vec::new();
    for seg in path.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                // A drive (`C:`) is a root, like `/`.
                if out
                    .last()
                    .is_some_and(|l| *l != ".." && !(abs && out.len() == 1 && l.ends_with(':')))
                {
                    out.pop();
                } else if !abs {
                    out.push("..");
                }
            }
            s => out.push(s),
        }
    }
    let joined = out.join("/");
    if path.starts_with('/') { format!("/{joined}") } else { joined }
}

/// Percent-decodes `s`; a malformed escape stays as written.
fn decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%'
            && let Some(hex) = s.get(i + 1..i + 3)
            && let Ok(v) = u8::from_str_radix(hex, 16)
        {
            out.push(v);
            i += 3;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    String::from_utf8(out).unwrap_or_else(|_| s.to_owned())
}

/// A heading's GitHub-style anchor: lowercase, spaces to `-`, punctuation dropped.
pub fn slug(text: &str) -> String {
    text.trim()
        .chars()
        .filter_map(|c| match c {
            ' ' => Some('-'),
            c if c.is_alphanumeric() || c == '-' || c == '_' => Some(c.to_ascii_lowercase()),
            _ => None,
        })
        .flat_map(char::to_lowercase)
        .collect()
}

/// The folder links in session `sid` resolve against.
pub fn link_directory(host: &dyn FilesHost, sid: &str) -> String {
    host.session_directory(sid)
        .filter(|d| !d.is_empty())
        .unwrap_or_else(|| host.draft_directory())
        .to_owned()
}

/// Follows `href` clicked in the open session's timeline (a reply, a note, a card).
pub fn open_reply_link(host: &mut dyn FilesHost, href: &str, tx: &mut Tx) {
    // Refuse raw UNC/SMB/device paths BEFORE anything touches the filesystem or invokes plugins.
    if browser_policy::is_raw_unc_href(href) {
        host.toast_say(opencode_i18n::t!("rune.files.link.outside").to_owned(), href.trim().to_owned());
        return;
    }
    if tx.button() == rune::PointerButton::Middle {
        return open_middle_link(host, href, tx);
    }
    // Inline code naming a file (`code_paths`).
    if let Some(code) = href.strip_prefix(opencode_ui::style::CODE_LINK) {
        return code_paths::open_code_path(host, code, tx);
    }
    // A plugin's link handler (`LinkHandler`) takes the links it matches first.
    if host.plugin_open_link(href, tx) {
        return;
    }
    if let Some(sid) = host.active_session_id().map(str::to_owned) {
        return open_link(host, &sid, None, href, tx);
    }
    // A new session's page: no side panel to open a file in. Never execute via OS default app;
    // at most reveal a local fixed-drive path in the file manager, routing the draft directory
    // through the same process-wide physical root pin table.
    let draft_dir = host.draft_directory().to_owned();
    let draft_phys = host.draft_physical_directory();
    let home = host.home_directory().into_owned();
    let is_file_uri = href.trim().get(..5).is_some_and(|s| s.eq_ignore_ascii_case("file:"));
    let mut classified = classify(href, "", &draft_dir, &home);
    if matches!(classified, Link::Outside { .. })
        && let Some(ref phys) = draft_phys
        && phys != &draft_dir
    {
        let phys_link = classify(href, "", phys, &home);
        if matches!(phys_link, Link::Inside { .. }) {
            classified = phys_link;
        }
    }
    match classified {
        Link::Web(_) => open_web(host, None, href, false, tx),
        Link::Heading(_) => {}
        Link::Inside { path, .. } if !is_file_uri => {
            let Some(root) = draft_phys else {
                host.toast_say(opencode_i18n::t!("rune.files.link.outside").to_owned(), path);
                return;
            };
            let abs = opencode_kit::path::absolute(&root, &path);
            reveal_outside_file(host, &abs);
        }
        Link::Outside { path } if !is_file_uri => reveal_outside_file(host, &path),
        Link::Inside { .. } | Link::Outside { .. } | Link::Skill { .. } => {
            host.toast_say(opencode_i18n::t!("rune.files.link.outside").to_owned(), href.trim().to_owned());
        }
    }
}

/// A middle click on `href`: routes web and mail addresses through browser URL policy (always
/// external for valid `http:`/`https:`, mail client for safe `mailto:`, copy-only for `mailto:`
/// with attachment params, never `file:` or credentialed URLs).
fn open_middle_link(host: &mut dyn FilesHost, href: &str, tx: &mut Tx) {
    let web = ["http:", "https:", "mailto:"]
        .iter()
        .any(|s| href.get(..s.len()).is_some_and(|p| p.eq_ignore_ascii_case(s)));
    if web {
        let sid = host.active_session_id().map(str::to_owned);
        open_web(host, sid.as_deref(), href, false, tx);
    }
}

fn open_web(host: &mut dyn FilesHost, sid: Option<&str>, url: &str, from_file: bool, tx: &mut Tx) {
    host.route_web_link(sid, url, from_file, tx);
}

/// Follows `href` clicked in session `sid`'s timeline, or in its file `from` (`Some`) rendered
/// in the viewer, whose folder relative links resolve against.
pub fn open_link(host: &mut dyn FilesHost, sid: &str, from: Option<&str>, href: &str, tx: &mut Tx) {
    // Refuse raw UNC/SMB/device paths BEFORE anything touches the filesystem or invokes plugins.
    if browser_policy::is_raw_unc_href(href) {
        host.toast_say(opencode_i18n::t!("rune.files.link.outside").to_owned(), href.trim().to_owned());
        return;
    }
    if tx.button() == rune::PointerButton::Middle {
        return open_middle_link(host, href, tx);
    }
    if from.is_some() && host.plugin_open_link(href, tx) {
        return;
    }
    let base = from.map_or("", opencode_kit::path::parent);
    let dir = link_directory(host, sid);
    let home = host.home_directory().into_owned();
    let mut classified = classify(href, base, &dir, &home);
    if matches!(classified, Link::Outside { .. })
        && let Some(phys) = host.session_physical_directory(sid)
        && phys != dir
    {
        let phys_link = classify(href, base, &phys, &home);
        if matches!(phys_link, Link::Inside { .. }) {
            classified = phys_link;
        }
    }
    match classified {
        Link::Web(url) => {
            if !host.try_open_artifact_link(sid, &url) {
                open_web(host, Some(sid), href, from.is_some(), tx);
            }
        }
        Link::Heading(h) => {
            if let Some(f) = from {
                markdown::scroll_to_heading(host, sid, f, &h, tx);
            }
        }
        Link::Inside { path, lines, heading } if from == Some(path.as_str()) && lines.is_none() => {
            if let Some(h) = heading {
                markdown::scroll_to_heading(host, sid, &path, &h, tx);
            }
        }
        Link::Inside { path, lines: None, .. } if host.opens_as_page(&path) => {
            host.open_workspace_preview(sid, &path, tx);
        }
        Link::Inside { path, lines, heading } => open_file_link(host, sid, &path, lines, heading, tx),
        // Never hand a file outside the workspace to the OS shell (`ours.md` §4): it opens in
        // the viewer, a raw `file://` URL too.
        Link::Outside { path } => open_outside_file(host, sid, &path, tx),
        Link::Skill { path, lines } => open_file_link(host, sid, &path, lines, None, tx),
    }
}

/// A file outside the session's folder opens in the viewer as Desktop's does (`file/renderer.tsx`
/// `open` -> `openSource`): read through the server with its own folder as the Location, so it
/// works on a remote server too and nothing is handed to the OS. Refused before the server is
/// asked anything: a UNC or device path, and one `~` could not expand; an HTML file gets
/// Desktop's words (`preview.outside`). A video stays with the file manager, where a file
/// outside the folder plays.
fn open_outside_file(host: &mut dyn FilesHost, sid: &str, path: &str, tx: &mut Tx) {
    let drive = path.as_bytes().get(1) == Some(&b':');
    if !is_absolute(path) || browser_policy::is_unc_or_device_path_for(path, drive) {
        host.toast_say(opencode_i18n::t!("rune.files.link.outside").to_owned(), path.to_owned());
        return;
    }
    let ext = path.rsplit_once('.').map_or("", |(_, e)| e);
    if ["html", "htm"].iter().any(|e| ext.eq_ignore_ascii_case(e)) {
        host.toast_fail(opencode_i18n::t!("plugin.file.preview.outside").to_owned(), String::new());
        return;
    }
    if crate::video::is_video(path) {
        return reveal_outside_file(host, path);
    }
    open_file_link(host, sid, path, None, None, tx);
}

/// Opens `path` (in session `sid`'s folder) in the viewer once it's known to exist, at
/// `lines` or `heading`; says so when it doesn't.
fn open_file_link(
    host: &mut dyn FilesHost,
    sid: &str,
    path: &str,
    lines: Option<(u32, u32)>,
    heading: Option<String>,
    tx: &mut Tx,
) {
    // No `tx.instant()` here: the side region this opens flies on `SIDE_SPRING`, and a tab
    // switch in an open one lands at once (`show_pane_tab`).
    host.ensure_session_review(sid);
    {
        let ws = host.files_mut().sessions.entry(sid.to_owned()).or_default();
        // Ask again for a file that wasn't there, or whose answer was lost.
        if ws.docs.get(path).is_some_and(|d| !d.content().has_content()) {
            ws.invalidate_document(path);
        }
    }
    ops::ensure_file_loaded(host, sid, path);
    let pending = {
        let Some(ws) = host.files_mut().sessions.get_mut(sid) else { return };
        let link = LinkOpen { lines, heading };
        match ws.docs.get(path) {
            Some(d) if d.content().is_loading() => {
                ws.link_opens.insert(path.to_owned(), link);
                None
            }
            _ => Some(link),
        }
    };
    if let Some(link) = pending {
        finish_file_link(host, sid, path, link, tx);
    }
}

/// The file a link asked for has loaded: opens its tab, or says it isn't there.
pub(crate) fn finish_file_link(host: &mut dyn FilesHost, sid: &str, path: &str, link: LinkOpen, tx: &mut Tx) {
    let missing = {
        let pane = host.files_mut();
        let waker = pane.waker.clone();
        let Some(ws) = pane.sessions.get_mut(sid) else { return };
        if ws.docs.get(path).is_none_or(|d| d.content().is_missing()) {
            ws.invalidate_document(path);
            if ws.known_files.remove(path) {
                ws.reindex(waker.as_ref());
            }
            ws.rebuild_tree();
            true
        } else {
            // A line is in the source: a rendered file shows it there.
            if link.lines.is_some()
                && (markdown::is_markdown(path) || ws.html.contains_key(path) || ws.tables.contains_key(path))
            {
                ws.close_preview(path);
                ws.md.entry(path.to_owned()).or_default().mode.step(markdown::PreviewModeEvent::ShowSource);
            }
            false
        }
    };
    if missing {
        tx.instant();
        let title = if is_bare_word(path) {
            opencode_i18n::t!("rune.files.link.nowhere")
        } else {
            opencode_i18n::t!("rune.files.link.notFound")
        };
        host.toast_say(title.to_owned(), path.to_owned());
        return;
    }
    // Before the tab opens: a scroll to a source line asks for `tx.instant()`, which would land
    // the side region's flight. The scroll applies to the view this commit mounts either way.
    if let Some(h) = link.heading {
        markdown::scroll_to_heading(host, sid, path, &h, tx);
    }
    ops::open_workspace_file_for_session(host, sid, path, false, link.lines, false, tx);
}

/// A file outside the session's folder: NEVER executes or opens with the default OS app, and
/// NEVER calls `exists()` / `metadata()` / `canonicalize()` on UNC or device paths. At most
/// reveals a verified local fixed-drive path in the file manager (`SHOpenFolderAndSelectItems` /
/// `open -R`).
pub(crate) fn reveal_outside_file(host: &mut dyn FilesHost, path: &str) {
    let Some(clean) = browser_policy::safe_local_reveal_path(path) else {
        host.toast_say(opencode_i18n::t!("rune.files.link.outside").to_owned(), path.to_owned());
        return;
    };
    if !host.can_reveal_outside() {
        host.toast_say(opencode_i18n::t!("rune.files.link.outside").to_owned(), clean);
        return;
    }
    host.open_in_reveal_abs(&clean);
}
