//! Asks the server to watch each open file, as Desktop's file model does
//! (`packages/app/src/workspaces/files/model.tsx`, `watch` and `unwatch`): when a file tab opens
//! and each time one shows (the server keeps a watch 24 hours from the latest ask), for every
//! open file tab when the stream connects or comes back (a restarted server forgets its
//! watches), and no more when the last tab holding it closes or its session is dropped. The
//! server then publishes `filesystem.changed` for it. A server keeps at most 256 watches a
//! folder and drops the one that would expire first for a new one: that file hears nothing
//! more until its tab shows again. Errors are ignored, as Desktop's are (an older server has no
//! such route). Live servers only; a path outside the session's folder isn't watched.

use std::collections::BTreeSet;

use opencode_search::query::percent_encode;
use serde_json::json;

use crate::FilesHost;

/// `(folder, path)`: the sessions whose open tabs hold its watch.
pub type Watches = std::collections::HashMap<(String, String), BTreeSet<String>>;

/// `sid`'s tab for `path` opened or showed: (re)arm its watch. False when it can't be asked yet
/// (its session's folder isn't known) or at all (a fixture, a path outside the folder).
pub fn watch(host: &mut dyn FilesHost, sid: &str, path: &str) -> bool {
    let Some(dir) = directory(host, sid, path) else { return false };
    host.files_mut()
        .watches
        .entry((dir.clone(), path.to_owned()))
        .or_default()
        .insert(sid.to_owned());
    send(host, "POST", &dir, path);
    true
}

/// `sid`'s tab for `path` closed: the watch stops once no session's tab holds it.
pub fn unwatch(host: &mut dyn FilesHost, sid: &str, path: &str) {
    let Some(dir) = directory(host, sid, path) else { return };
    let key = (dir.clone(), path.to_owned());
    let empty = {
        let Some(holders) = host.files_mut().watches.get_mut(&key) else { return };
        holders.remove(sid);
        holders.is_empty()
    };
    if empty {
        host.files_mut().watches.remove(&key);
        send(host, "DELETE", &dir, path);
    }
}

/// `sid`'s open files were dropped (its session closed): their watches stop.
pub fn forget_session(host: &mut dyn FilesHost, sid: &str) {
    host.files_mut().forget_video_auth_session(sid);
    let held: Vec<(String, String)> = host
        .files()
        .watches
        .iter()
        .filter(|(_, sids)| sids.contains(sid))
        .map(|(k, _)| k.clone())
        .collect();
    for (dir, path) in held {
        let key = (dir.clone(), path.clone());
        let empty = host.files_mut().watches.get_mut(&key).is_some_and(|holders| {
            holders.remove(sid);
            holders.is_empty()
        });
        if empty {
            host.files_mut().watches.remove(&key);
            send(host, "DELETE", &dir, &path);
        }
    }
}

/// The stream connected or came back: every open file tab's watch again, the shown one first.
pub fn connected(host: &mut dyn FilesHost) {
    for (sid, path) in open_tabs(host) {
        watch(host, &sid, &path);
    }
}

/// `filesystem.changed`: an open file changed on disk (Desktop's `invalidateFromWatcher`,
/// `packages/app/src/workspaces/files/watcher.ts`), read again at once, keeping the view, for
/// each session holding it: in the session on screen whether its tab shows or not, in one off
/// screen when that session shows (`reload`). Matched by its absolute path inside each session's
/// folder, whatever the separators (Desktop compares them as they come, so on Windows a file in
/// a folder never matches). `.git` isn't a file anyone opens. A file gone (`unlink`) from a
/// folder the tree has listed lists that folder again, as Desktop's does.
pub fn changed(host: &mut dyn FilesHost, data: &serde_json::Value) {
    let Some(file) = data["file"].as_str() else { return };
    let mut reads = Vec::new();
    let mut lists = Vec::new();
    for (sid, ws) in host.files().sessions.iter() {
        let Some(path) = host
            .session_directory(sid)
            .and_then(|dir| super::data::relative_path(file, dir))
        else {
            continue;
        };
        if path == ".git" || path.starts_with(".git/") {
            continue;
        }
        let parent = path.rsplit_once('/').map_or("", |(dir, _)| dir).to_owned();
        if data["event"] == "unlink" && ws.can_relist(&parent) {
            lists.push((sid.clone(), parent));
        }
        if ws.docs.contains_key(&path) {
            reads.push((sid.clone(), path));
        }
    }
    for (sid, path) in reads {
        reload(host, &sid, &path);
    }
    for (sid, dir) in lists {
        crate::ops::relist_workspace_dir(host, &sid, &dir);
    }
}

/// `sid`'s `path` may have changed: read it again, keeping what shows meanwhile. A video's
/// player opens it again (its file changed under it). A session off screen reads nothing: its
/// file is marked stale and reads when the session shows (`shown`), so an agent rewriting many
/// files costs no reads for files nobody is looking at.
pub fn reload(host: &mut dyn FilesHost, sid: &str, path: &str) {
    if host.active_session_id() != Some(sid) {
        host.files_mut().reads.outdate(sid, path);
        return;
    }
    super::trace::changed(sid, path);
    if super::video::is_video(path) {
        let pane = host.files_mut();
        pane.reads.clear(sid, path);
        pane.forget_video_auth(sid, path);
        if let Some(ws) = pane.sessions.get_mut(sid) {
            ws.videos.remove(path);
            ws.docs.remove(path);
        }
        crate::ops::ensure_file_loaded(host, sid, path);
        return;
    }
    crate::ops::read_file(host, sid, path, true);
}

/// Session `sid` came on screen: its files that changed while it was off read again, silently,
/// the shown tab first.
pub fn shown(host: &mut dyn FilesHost, sid: &str) {
    let mut stale = host.files().reads.stale_in(sid);
    if stale.is_empty() {
        return;
    }
    let first = host.active_file_tab().and_then(|(_, path)| path);
    stale.sort_by_key(|p| Some(p) != first.as_ref());
    for path in stale {
        reload(host, sid, &path);
    }
}

/// The stream came back: a change made while the server was away went unreported, so every
/// open file reads again once, the shown one first (one read at a time per file, as in a burst).
pub fn reread(host: &mut dyn FilesHost) {
    for (sid, path) in open_tabs(host) {
        if host.files().sessions.get(&sid).is_some_and(|w| w.docs.contains_key(&path)) {
            reload(host, &sid, &path);
        }
    }
}

/// Every session's open file tabs, the shown one first.
pub fn open_tabs(host: &dyn FilesHost) -> Vec<(String, String)> {
    let shown = host.active_file_tab().and_then(|(sid, path)| Some((sid, path?)));
    let mut tabs: Vec<(String, String)> = shown.iter().cloned().collect();
    for (sid, path) in host.all_open_file_tabs() {
        if shown.as_ref() != Some(&(sid.clone(), path.clone())) {
            tabs.push((sid, path));
        }
    }
    tabs
}

/// The folder `sid`'s file `path` is in, when the server can watch it.
fn directory(host: &dyn FilesHost, sid: &str, path: &str) -> Option<String> {
    if !host.is_live() || path.is_empty() || crate::data::is_outside(path) {
        return None;
    }
    host.session_directory(sid)
        .filter(|d| !d.is_empty())
        .map(str::to_owned)
}

fn send(host: &dyn FilesHost, method: &'static str, directory: &str, path: &str) {
    let Some(server) = host.live_server().cloned() else { return };
    let path = path.to_owned();
    let url = format!("/api/experimental/fs/watch?location%5Bdirectory%5D={}", percent_encode(directory));
    drop(rune::task::spawn("file-watch", move || {
        drop(server.write(method, &url, Some(json!({ "path": path }))));
    }));
}
