//! Non-generic workspace file operations over `&dyn FilesHost` / `&mut dyn FilesHost`:
//! directory listing, file loading, opening files in tabs, sidebar filtering, search, and job execution.

use opencode_code_view::prepared::Prior;
use opencode_kit::client::Server;
use rune::Tx;
use serde_json::Value;

use crate::data::{
    fetch_live_dir, fetch_live_find, file_tab_key, load_fixture_workspace, parse_path_and_line,
    rank_file_query, try_fetch_live_dir,
};
use crate::{
    FilesHost, FilesJob, FilesReply, TreeLoadEffect, TreeLoadEvent, code_paths, delimited, html,
    narrow, video, watch,
};

/// Executes a [`FilesJob`] off the UI thread against the live [`Server`].
pub fn execute_job(server: &Server, job: FilesJob) -> FilesReply {
    match job {
        FilesJob::CheckCodePath { session_id, doc_path, code, directory, target } => {
            let exists = code_paths::execute_check_code_path(server, &directory, &target);
            FilesReply::CodePathChecked { session_id, doc_path, code, exists }
        }
        FilesJob::ResolveCodeLink { session_id, directory, path, code, lines } => {
            let target = code_paths::execute_resolve_code_link(server, &directory, &path);
            FilesReply::CodeLinkResolved { session_id, code, lines, target }
        }
    }
}

/// Applies a completed [`FilesReply`] back onto `host` on the UI thread.
pub fn apply_reply(host: &mut dyn FilesHost, reply: FilesReply, tx: &mut Tx) {
    match reply {
        FilesReply::CodePathChecked { session_id, doc_path, code, exists } => {
            code_paths::apply_code_path_checked(host, &session_id, &doc_path, code, exists, tx);
        }
        FilesReply::CodeLinkResolved { session_id, code, lines, target } => {
            code_paths::apply_code_link_resolved(host, &session_id, &code, lines, target, tx);
        }
    }
}

pub fn ensure_workspace_files(host: &mut dyn FilesHost, session_id: &str) {
    if host.files().sessions.get(session_id).is_some_and(|w| w.tree_load.is_ready()) {
        return;
    }
    let Some(srv) = host.live_server().cloned() else {
        let empty = Value::Null;
        let fixture = host.fixture_root().unwrap_or(&empty);
        let (files, contents) = load_fixture_workspace(fixture);
        let pane = host.files_mut();
        let wrap = pane.wrap;
        let waker = pane.waker.clone();
        let ws = pane.sessions.entry(session_id.to_owned()).or_default();
        ws.known_files = files;
        ws.reindex(waker.as_ref());
        ws.expanded_dirs.insert("src".into());
        ws.expanded_dirs.insert("src/net".into());
        for (path, text) in contents {
            ws.docs
                .entry(path.clone())
                .or_insert_with(|| wrap.doc(&path, &text));
        }
        ws.tree_load.step(TreeLoadEvent::Loaded);
        ws.rebuild_tree();
        return;
    };
    // A session restored before the server has listed it has no folder yet: an empty
    // one would list the server's own working folder. It's asked again as it shows.
    let Some(directory) = host
        .session_directory(session_id)
        .filter(|d| !d.is_empty())
        .map(str::to_owned)
    else {
        return;
    };
    if host
        .files()
        .sessions
        .get(session_id)
        .is_some_and(|w| !w.tree_load.is_idle())
    {
        return;
    }
    let pane = host.files_mut();
    let ws = pane.sessions.entry(session_id.to_owned()).or_default();
    if ws.tree_load.step(TreeLoadEvent::RequestLive) == TreeLoadEffect::FetchRoot {
        let sid = session_id.to_owned();
        pane.dir_fetch.request((sid, directory), move |(sid, dir)| {
            let entries = fetch_live_dir(&srv, &dir, "");
            (sid, String::new(), entries)
        });
    }
}

pub fn toggle_workspace_dir(host: &mut dyn FilesHost, dir_path: &str) {
    let Some(sid) = host.active_session_id().map(str::to_owned) else { return };
    let is_live = host.is_live();
    let need_fetch = {
        let ws = host.files_mut().sessions.entry(sid.clone()).or_default();
        if ws.expanded_dirs.contains(dir_path) {
            ws.expanded_dirs.remove(dir_path);
            ws.rebuild_tree();
            false
        } else {
            ws.expanded_dirs.insert(dir_path.to_owned());
            ws.rebuild_tree();
            is_live && !ws.listed_dirs.contains(dir_path)
        }
    };
    if need_fetch && let Some(srv) = host.live_server().cloned() {
        let directory = host.session_directory(&sid).unwrap_or_default().to_owned();
        let rel = dir_path.to_owned();
        host.files_mut()
            .dir_fetch
            .request((sid, directory, rel), move |(sid, dir, rel)| {
                let entries = fetch_live_dir(&srv, &dir, &rel);
                (sid, rel, entries)
            });
    }
}

/// Lists `sid`'s folder `dir` again (a file in it went), on a worker of its own, so it never
/// takes the place of a folder the person just opened.
pub fn relist_workspace_dir(host: &mut dyn FilesHost, sid: &str, dir: &str) {
    let Some(server) = host.live_server().cloned() else { return };
    let (sid, rel) = (sid.to_owned(), dir.to_owned());
    let directory = host.session_directory(&sid).unwrap_or_default().to_owned();
    let (sender, waker) = (host.files().relists.0.clone(), host.files().waker.clone());
    drop(rune::task::spawn("file-relist", move || {
        let Some(entries) = try_fetch_live_dir(&server, &directory, &rel) else { return };
        if sender.send((sid, rel, entries)).is_ok()
            && let Some(w) = waker
        {
            w();
        }
    }));
}

pub fn ensure_file_loaded(host: &mut dyn FilesHost, session_id: &str, path: &str) {
    ensure_workspace_files(host, session_id);
    let already = host
        .files()
        .sessions
        .get(session_id)
        .and_then(|w| w.docs.get(path))
        // One still loading asks again (it joins the read in flight), one whose read failed
        // reads again, and one a change left stale reads again.
        .is_some_and(|d| !matches!(d.content(), opencode_code_view::Content::Loading | opencode_code_view::Content::Failed { .. }))
        && !host.files().reads.stale(session_id, path);
    if already {
        return;
    }
    let Some(server) = host.live_server().cloned() else {
        let empty = Value::Null;
        let fixture = host.fixture_root().unwrap_or(&empty);
        let (_, contents) = load_fixture_workspace(fixture);
        let pane = host.files_mut();
        let wrap = pane.wrap;
        let waker = pane.waker.clone();
        let ws = pane.sessions.entry(session_id.to_owned()).or_default();
        // A file outside the folder, and a skill's virtual file, is no row of the tree.
        if !crate::data::is_outside(path) && ws.known_files.insert(path.to_owned()) {
            ws.reindex(waker.as_ref());
        }
        let mut doc = wrap.doc(path, "");
        ws.invalidate_document(path);
        if let Some(txt) = contents.get(path) {
            doc.set_text(txt);
            if let Some(table) = delimited::Table::parse(path, txt) {
                ws.tables.insert(path.to_owned(), table.into());
            }
            if let Some(page) = html::Page::parse(path, txt) {
                ws.html.insert(path.to_owned(), page);
            }
        } else {
            doc.set_missing();
        }
        ws.docs.insert(path.to_owned(), doc);
        ws.rebuild_tree();
        return;
    };

    // No folder yet (a session restored before the server listed it): the file shows
    // loading and reads once the folder is known (`OpenDocument` asks again).
    let Some(directory) = host
        .session_directory(session_id)
        .filter(|d| !d.is_empty())
        .map(str::to_owned)
    else {
        let pane = host.files_mut();
        let wrap = pane.wrap;
        pane.sessions
            .entry(session_id.to_owned())
            .or_default()
            .docs
            .entry(path.to_owned())
            .or_insert_with(|| wrap.doc(path, ""));
        return;
    };

    // A video streams into its player where it lies: nothing is read first. One that
    // can't play is read as any other file, and shows as the binary file it is. When a stale
    // reload arrives (e.g. a file changed while its session was off-screen), discard any prior
    // failed player or auth failure so a repaired/completed video retries cleanly.
    if video::is_video(path) && host.files().reads.stale(session_id, path) {
        let pane = host.files_mut();
        pane.forget_video_auth(session_id, path);
        if let Some(ws) = pane.sessions.get_mut(session_id) {
            ws.videos.remove(path);
        }
    }
    let failed = host
        .files()
        .sessions
        .get(session_id)
        .and_then(|w| w.videos.get(path))
        .is_some_and(video::FileVideo::failed)
        || host.files().video_auth_failed(session_id, path);
    if video::is_video(path) && !failed {
        let video_dir = if server.is_local() {
            host.session_physical_directory(session_id)
        } else {
            Some(directory.clone())
        };
        if let Some(source) = video_dir.as_deref().and_then(|d| video::source(Some(&server), d, path)) {
            let pane = host.files_mut();
            pane.reads.clear(session_id, path);
            let wrap = pane.wrap;
            let clip = video::FileVideo::new(&source, pane.waker.clone());
            let ws = pane.sessions.entry(session_id.to_owned()).or_default();
            let mut doc = wrap.doc(path, "");
            doc.set_binary(0);
            ws.docs.insert(path.to_owned(), doc);
            ws.videos.insert(path.to_owned(), clip);
            return;
        }
        // On a native local server, `source() == None` means confinement refused a symlink/junction
        // or missing file: never fall back to `/api/fs/read/...`. Remove any stale player and
        // insert a terminal Missing doc so OpenDocument stops re-requesting on every render.
        // On wasm32, `video::source` returns `None` for any authenticated server (local or remote),
        // which intentionally falls through to `read_file` below.
        #[cfg(not(target_arch = "wasm32"))]
        if server.is_local() {
            let pane = host.files_mut();
            pane.reads.clear(session_id, path);
            let wrap = pane.wrap;
            let ws = pane.sessions.entry(session_id.to_owned()).or_default();
            ws.videos.remove(path);
            let mut doc = wrap.doc(path, "");
            doc.set_missing();
            ws.docs.insert(path.to_owned(), doc);
            return;
        }
        // Password-protected remote server: never put `auth_token` in a URL; download via
        // `Authorization` header off the UI thread on native (on wasm32 fall through to `read_file`).
        #[cfg(not(target_arch = "wasm32"))]
        if server.has_auth()
            && let Some(dir) = video_dir
        {
            let pane = host.files_mut();
            pane.reads.clear(session_id, path);
            let wrap = pane.wrap;
            let waker = pane.waker.clone();
            if pane.auth_videos.start(&server, &dir, session_id, path, waker) {
                let ws = pane.sessions.entry(session_id.to_owned()).or_default();
                ws.docs.entry(path.to_owned()).or_insert_with(|| wrap.doc(path, ""));
                return;
            }
        }
    }

    // A stale file keeps its text while it reads again; a failed one shows loading.
    {
        let pane = host.files_mut();
        let wrap = pane.wrap;
        let ws = pane.sessions.entry(session_id.to_owned()).or_default();
        if !ws.docs.get(path).is_some_and(|d| !matches!(d.content(), opencode_code_view::Content::Loading | opencode_code_view::Content::Failed { .. })) {
            ws.docs.insert(path.to_owned(), wrap.doc(path, ""));
        }
    }
    read_file(host, session_id, path, false);
}

/// Reads `path` for `sid` unless a read runs already (`content`); `force` reads even when it
/// has loaded (it changed on disk, or while the server was away). Live servers only.
pub fn read_file(host: &mut dyn FilesHost, sid: &str, path: &str, force: bool) {
    if !host.is_live() {
        return;
    }
    let loaded = host
        .files()
        .sessions
        .get(sid)
        .and_then(|w| w.docs.get(path))
        .is_some_and(|d| !matches!(d.content(), opencode_code_view::Content::Loading | opencode_code_view::Content::Failed { .. }));
    if let Some(attempt) = host.files_mut().reads.ask(sid, path, force, loaded) {
        start_read(host, sid, path, attempt);
    }
}

pub fn start_read(host: &mut dyn FilesHost, sid: &str, path: &str, attempt: u64) {
    let Some(server) = host.live_server().cloned() else { return };
    let Some(directory) = host
        .session_directory(sid)
        .filter(|d| !d.is_empty())
        .map(str::to_owned)
    else {
        host.files_mut().reads.abandon(sid, path);
        return;
    };
    let (find, prior) = {
        let doc = host.files().sessions.get(sid).and_then(|w| w.docs.get(path));
        let find = doc.and_then(|d| d.find_query());
        let prior = doc.map_or_else(|| Prior::initial(path), |d| d.prior());
        (find, prior)
    };
    host.files_mut()
        .reads
        .start(&server, &directory, sid, path, attempt, find, prior);
}

/// Opens workspace-relative `path` (local or remote) as an isolated `oc-preview://` tab.
pub fn open_html_in_browser(host: &mut dyn FilesHost, sid: &str, path: &str, tx: &mut Tx) {
    host.open_workspace_preview(sid, path, tx);
}

/// Opens `path` from the Files sidebar tree/filter: routes workspace HTML files to the browser
/// pane when [`FilesHost::opens_as_page`] is true (`packages/gui-plugins/src/file/renderer.tsx`),
/// otherwise opens the file in the Files viewer.
pub fn open_workspace_file_or_page(host: &mut dyn FilesHost, path: &str, preview: bool, tx: &mut Tx) {
    let Some(sid) = host.active_session_id().map(str::to_owned) else { return };
    if host.opens_as_page(path) {
        open_html_in_browser(host, &sid, path, tx);
    } else {
        open_workspace_file_for_session(host, &sid, path, preview, None, true, tx);
    }
}

pub fn open_workspace_file(
    host: &mut dyn FilesHost,
    path: &str,
    preview: bool,
    line: Option<u32>,
    tx: &mut Tx,
) {
    open_workspace_file_at(host, path, preview, line.map(|n| (n, n)), tx);
}

/// Opens `path` with lines `start..=end` selected and scrolled into view.
pub fn open_workspace_file_at(
    host: &mut dyn FilesHost,
    path: &str,
    preview: bool,
    lines: Option<(u32, u32)>,
    tx: &mut Tx,
) {
    let Some(sid) = host.active_session_id().map(str::to_owned) else { return };
    open_workspace_file_for_session(host, &sid, path, preview, lines, true, tx);
}

pub fn open_workspace_file_for_session(
    host: &mut dyn FilesHost,
    sid: &str,
    path: &str,
    preview: bool,
    lines: Option<(u32, u32)>,
    steal_focus: bool,
    tx: &mut Tx,
) {
    // A skill's virtual file is no file of the folder the browser pane can show.
    if lines.is_none() && !crate::data::is_virtual_skill(path) && host.opens_as_page(path) {
        host.open_workspace_preview(sid, path, tx);
        return;
    }
    open_workspace_file_in_viewer(host, sid, path, preview, lines, steal_focus, false, tx);
}

/// Opens `path` directly in the Files code viewer in forced source mode (never bouncing back to
/// the browser preview pane). Used by the browser preview menu's `"Show the file's code"` action.
pub fn open_workspace_file_source(
    host: &mut dyn FilesHost,
    sid: &str,
    path: &str,
    preview: bool,
    lines: Option<(u32, u32)>,
    steal_focus: bool,
    tx: &mut Tx,
) {
    open_workspace_file_in_viewer(host, sid, path, preview, lines, steal_focus, true, tx);
}

#[allow(clippy::too_many_arguments)]
fn open_workspace_file_in_viewer(
    host: &mut dyn FilesHost,
    sid: &str,
    path: &str,
    preview: bool,
    lines: Option<(u32, u32)>,
    steal_focus: bool,
    force_source: bool,
    tx: &mut Tx,
) {
    ensure_file_loaded(host, sid, path);
    // A file outside the session's folder (`links::open_outside_file`), and a skill's virtual
    // file, is not in its tree and is not watched.
    let outside = crate::data::is_outside(path);
    if !outside {
        watch::watch(host, sid, path);
    }
    let ws = host.files_mut().sessions.entry(sid.to_owned()).or_default();
    if force_source {
        ws.close_preview(path);
        ws.md.entry(path.to_owned()).or_default().mode.step(crate::markdown::PreviewModeEvent::ShowSource);
    }
    ws.narrow = ws.narrow.step(narrow::NarrowFilesEvent::Opened);
    ws.push_recent(path);
    // Ensure parent folders in the tree are expanded so the active file highlights in context.
    let parts: Vec<&str> = if outside { Vec::new() } else { path.split('/').collect() };
    for i in 0..parts.len().saturating_sub(1) {
        ws.expanded_dirs.insert(parts[..=i].join("/"));
    }
    ws.rebuild_tree();
    ws.tree.select(path);

    if let Some((start, end)) = lines {
        if let Some(doc) = ws.docs.get_mut(path)
            && !doc.content().is_loading()
        {
            let scroll_key = format!("file-code-{sid}-{path}-scroll");
            doc.scroll_to_lines(&scroll_key, start, end, tx);
        } else {
            ws.pending_jump.insert(path.to_owned(), (start, end));
        }
    }
    // Last, after `scroll_to_lines`'s `tx.instant()`: a side region this opens flies on
    // `SIDE_SPRING` (`show_pane_tab`). Removes the transient `file:open` tab once a concrete
    // file is picked (`file/renderer.tsx:128`).
    host.open_side_tab(sid, &file_tab_key(path), preview, steal_focus, true, tx);
}

pub fn filtered_sidebar_files(host: &dyn FilesHost, session_id: &str) -> Vec<String> {
    filtered_files(host, session_id, &host.files().filter.text)
}

/// The files of `session_id`'s workspace whose path holds `query`, best first: the server's
/// finds, then the files the tree has listed.
pub fn filtered_files(host: &dyn FilesHost, session_id: &str, query: &str) -> Vec<String> {
    let q = query.trim();
    let Some(ws) = host.files().sessions.get(session_id) else {
        return Vec::new();
    };
    let mut pool: Vec<String> = ws.search.found().to_vec();
    let (clean, _) = parse_path_and_line(q);
    let local = ws.local_files(clean.trim(), 100);
    if pool.is_empty() {
        return local;
    }
    for f in local {
        if !pool.contains(&f) {
            pool.push(f);
        }
    }
    rank_file_query(&pool, q)
}

pub fn submit_files_filter(host: &mut dyn FilesHost, tx: &mut Tx) {
    let Some(sid) = host.active_session_id().map(str::to_owned) else { return };
    let filtered = filtered_sidebar_files(host, &sid);
    let pick = host
        .files()
        .sessions
        .get(&sid)
        .and_then(|w| w.highlighted_filter_path.clone())
        .filter(|p| filtered.contains(p))
        .or_else(|| filtered.first().cloned());
    if let Some(path) = pick {
        host.files_mut().filter.focused = false;
        open_workspace_file_or_page(host, &path, false, tx);
    }
}

pub fn request_sidebar_file_search(host: &mut dyn FilesHost) {
    let Some(sid) = host.active_session_id().map(str::to_owned) else { return };
    let q = host.files().filter.text.clone();
    request_file_search(host, &sid, &q);
}

/// Asks the server for `session_id`'s files that match `query`, once per query.
pub fn request_file_search(host: &mut dyn FilesHost, session_id: &str, query: &str) {
    let (sid, q) = (session_id.to_owned(), query.trim().to_owned());
    if q.is_empty() {
        return;
    }
    let Some(srv) = host.live_server().cloned() else { return };
    let asked = {
        let ws = host.files_mut().sessions.entry(sid.clone()).or_default();
        ws.search.ask(&q)
    };
    if !asked {
        return;
    }
    let directory = host.session_directory(&sid).unwrap_or_default().to_owned();
    host.files_mut()
        .find_fetch
        .request((sid, directory, q), move |(sid, dir, q)| {
            let found = fetch_live_find(&srv, &dir, &q, 100);
            (sid, q, found)
        });
}
