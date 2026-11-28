//! `Workspace:OpenTree`, `Workspace:OpenDocument` and `ui.codeView`: the Files panel's workspace
//! tree and open file, read from the app's file engine (`crate::files`), which lists folders,
//! loads files, highlights them and finds in them. A tree or a document belongs to the session
//! whose folder it reads: the routed one, else the first open in that folder.

use std::hash::{Hash, Hasher};

use mlua::{Lua, Table, Value};
use rune::{El, KeyEvent, Tx, col};
use rune_luau::NodeArgs;

use super::super::cx::{self, Dep};
use super::super::services::{method, with_app};
use super::{handle_field, object, p};
use crate::App;

/// `Workspace:OpenTree` and `Workspace:OpenDocument`.
pub fn build(lua: &Lua, t: &Table) -> mlua::Result<()> {
    method(lua, t, "OpenTree", |lua, directory: String| {
        let sid = with_app("Workspace:OpenTree", |app| {
            let sid = session_for(app, &directory).ok_or_else(|| mlua::Error::runtime(format!("Workspace:OpenTree: no session is open in {directory}")))?;
            // Listed once its session's folder is known (an empty one would list the server's own).
            if !app.kernel.slots.get(&crate::files::FILES).sessions.get(&sid).is_some_and(|w| w.tree_load.is_ready()) && app.session(&sid).is_some_and(|i| !i.directory.is_empty()) {
                let id = sid.clone();
                app.later(Box::new(move |s: &mut App, tx: &mut Tx| {
                    tx.instant();
                    s.ensure_workspace_files(&id);
                }));
            }
            Ok(sid)
        })?;
        let fields = [("kind", Value::String(lua.create_string("workspace")?)), ("sessionId", Value::String(lua.create_string(&sid)?)), ("Root", Value::String(lua.create_string(&directory)?))];
        object(lua, "FileTreeModel", &fields, super::tree::methods(lua)?)
    })?;
    method(lua, t, "OpenDocument", |lua, path: String| {
        let (sid, path) = with_app("Workspace:OpenDocument", |app| {
            let sid = app.active_session_id().ok_or_else(|| mlua::Error::runtime("Workspace:OpenDocument: no session is open"))?.to_owned();
            // The path the file's tab carries (`plugins/file/panel.luau` `panel.path`): a file
            // outside the folder keeps the `/` its absolute path starts at, and a skill's virtual
            // file its `skill://<id>/<file>` whole.
            let path = crate::files::data::document_path(&path);
            // A file not read yet, one still loading with no read running (its session's folder
            // wasn't known), or one a failed refresh left stale, reads as it shows.
            let files = app.kernel.slots.get(&crate::files::FILES);
            let doc = files.sessions.get(&sid).and_then(|w| w.docs.get(&path));
            let known = app.session(&sid).is_some_and(|i| !i.directory.is_empty());
            let waiting = known && doc.is_some_and(|d| d.content().is_loading()) && !files.reads.running(&sid, &path) && !files.video_loading(&sid, &path);
            if doc.is_none() || waiting || (known && files.reads.stale(&sid, &path)) {
                let (id, at) = (sid.clone(), path.clone());
                app.later(Box::new(move |s: &mut App, tx: &mut Tx| {
                    tx.instant();
                    s.ensure_file_loaded(&id, &at);
                }));
            }
            Ok((sid, path))
        })?;
        let fields = [("Path", Value::String(lua.create_string(&path)?)), ("sessionId", Value::String(lua.create_string(&sid)?))];
        object(lua, "CodeDocument", &fields, document(lua)?)
    })?;
    method(lua, t, "OpenFile", |lua, (sid, path, options): (String, String, Value)| {
        let caller = lua
            .app_data_ref::<std::rc::Rc<super::super::vm::Vmx>>()
            .map(|v| v.half.id.to_string());
        #[cfg(not(target_arch = "wasm32"))]
        let (tool_sid, in_flight) = super::super::commands::current_tool_scope(lua);
        #[cfg(target_arch = "wasm32")]
        let (tool_sid, in_flight): (Option<Option<String>>, Vec<String>) = (None, Vec::new());
        let o: opencode_plugin_spec::OpenFileOptions = if options.is_nil() { Default::default() } else { mlua::FromLua::from_lua(options, lua).and_then(|d: rune_luau::Data| d.decode().map_err(mlua::Error::runtime))? };
        let lines = o.start_line.map(|a| {
            let a = a.max(1) as u32;
            (a, o.end_line.map_or(a, |b| (b.max(1) as u32).max(a)))
        });
        let preview = o.preview == Some(true);
        with_app("Workspace:OpenFile", |app| {
            let dir = app.session(&sid).map(|i| i.directory.clone()).unwrap_or_default();
            let Some(path) = crate::tool_open::relative(&path, &dir) else {
                return Err(mlua::Error::runtime(format!("Workspace:OpenFile: path '{path}' is outside the session folder")));
            };
            #[cfg(not(target_arch = "wasm32"))]
            if app.can_reveal_outside() && app.live() && !dir.is_empty() {
                let abs = opencode_kit::path::absolute(&dir, &path);
                if browser_policy::verify_within_root(&abs, &dir, false).is_none() {
                    return Err(mlua::Error::runtime(format!("Workspace:OpenFile: path '{path}' is outside the session folder")));
                }
            }
            let is_builtin = caller.as_deref().is_some_and(|id| app.plugins.is_builtin_plugin(id));
            let in_tool = tool_sid.is_some() || (!is_builtin && !in_flight.is_empty());
            if let Some(ref caller_sid) = tool_sid
                && caller_sid.as_deref() != Some(sid.as_str())
            {
                return Err(mlua::Error::runtime(
                    "Workspace:OpenFile cannot target another session inside a tool call.",
                ));
            }
            // Any tool-scoped call (built-in `file` `open_file` included) is an agent request:
            // hidden/secret HTML entries are refused synchronously and allowed HTML previews are
            // marked `agent_driven` (`notes/review-rd281.md` PR 2 2nd read #2).
            #[cfg(not(target_arch = "wasm32"))]
            if is_builtin && in_tool && lines.is_none() && app.opens_as_page(&path) {
                match app.resolve_workspace_file_target(&sid, &path, true) {
                    crate::browser::preview::PreviewTarget::Preview { display_url, spec } => {
                        app.later(Box::new(move |s: &mut App, tx: &mut Tx| {
                            tx.instant();
                            s.open_resolved_preview_tab(
                                Some(&sid),
                                &display_url,
                                spec,
                                None,
                                true,
                                Some(browser_pane::TabDriver::Agent),
                            );
                        }));
                        return Ok(());
                    }
                    crate::browser::preview::PreviewTarget::Refused(_)
                    | crate::browser::preview::PreviewTarget::NotPreview => {
                        return Err(mlua::Error::runtime(
                            "Hidden or secret files cannot be opened as a browser preview by an agent.",
                        ));
                    }
                }
            }
            app.later(Box::new(move |s: &mut App, tx: &mut Tx| {
                tx.instant();
                let builtin_now = caller.as_deref().is_some_and(|id| s.plugins.is_builtin_plugin(id));
                if !builtin_now {
                    // Third-party plugins always open workspace files in the Files source viewer
                    // on the allowed session only — never minting an `oc-preview://` grant
                    // (`notes/review-rd281.md` PR 2 Blocker #1).
                    let allowed_sid = tool_sid
                        .as_ref()
                        .map(|c| c.as_deref() == Some(sid.as_str()))
                        .unwrap_or_else(|| s.active_session_id() == Some(sid.as_str()));
                    if allowed_sid {
                        s.open_workspace_file_source(&sid, &path, preview, lines, false, tx);
                    }
                    return;
                }
                if in_tool {
                    // Tool-scoped non-preview open (e.g. non-HTML, line-ranged, or older engine):
                    // open strictly in Files source viewer so it can never mint a non-agent preview.
                    s.open_workspace_file_source(&sid, &path, preview, lines, false, tx);
                    return;
                }
                s.open_workspace_file_for_session(&sid, &path, preview, lines, false, tx);
                // When opened explicitly via Workspace:OpenFile (for example browser's "View source"),
                // show raw HTML source rather than Rune's simplified static HTML reader.
                if std::path::Path::new(&path)
                    .extension()
                    .is_some_and(|e| e.eq_ignore_ascii_case("html") || e.eq_ignore_ascii_case("htm"))
                    && let Some(ws) = s.kernel.slots.get_mut(&crate::files::FILES).sessions.get_mut(&sid)
                {
                    ws.md.entry(path.clone()).or_default().mode.step(crate::files::markdown::PreviewModeEvent::ShowSource);
                }
            }));
            Ok(())
        })
    })?;
    method(lua, t, "OpenFileSource", |lua, (sid, path): (String, String)| {
        let caller = lua
            .app_data_ref::<std::rc::Rc<super::super::vm::Vmx>>()
            .map(|v| v.half.id.to_string());
        #[cfg(not(target_arch = "wasm32"))]
        let (tool_sid, _) = super::super::commands::current_tool_scope(lua);
        #[cfg(target_arch = "wasm32")]
        let tool_sid: Option<Option<String>> = None;
        with_app("Workspace:OpenFileSource", |app| {
            let dir = app.session(&sid).map(|i| i.directory.clone()).unwrap_or_default();
            let Some(path) = crate::tool_open::relative(&path, &dir) else {
                return Err(mlua::Error::runtime(format!("Workspace:OpenFileSource: path '{path}' is outside the session folder")));
            };
            #[cfg(not(target_arch = "wasm32"))]
            if app.can_reveal_outside() && app.live() && !dir.is_empty() {
                let abs = opencode_kit::path::absolute(&dir, &path);
                if browser_policy::verify_within_root(&abs, &dir, false).is_none() {
                    return Err(mlua::Error::runtime(format!("Workspace:OpenFileSource: path '{path}' is outside the session folder")));
                }
            }
            if let Some(ref caller_sid) = tool_sid
                && caller_sid.as_deref() != Some(sid.as_str())
            {
                return Err(mlua::Error::runtime(
                    "Workspace:OpenFileSource cannot target another session inside a tool call.",
                ));
            }
            app.later(Box::new(move |s: &mut App, tx: &mut Tx| {
                let is_builtin = caller.as_deref().is_some_and(|id| s.plugins.is_builtin_plugin(id));
                if !is_builtin && tool_sid.is_none() && s.active_session_id() != Some(sid.as_str()) {
                    return;
                }
                tx.instant();
                s.open_workspace_file_source(&sid, &path, false, None, true, tx);
            }));
            Ok(())
        })
    })
}

fn read_doc<R>(this: &Table, what: &str, f: impl FnOnce(&App, &str, &str) -> mlua::Result<R>) -> mlua::Result<R> {
    let (sid, path): (String, String) = (this.raw_get("sessionId")?, this.raw_get("Path")?);
    cx::read(Dep::FilesDoc(sid.clone(), path.clone()));
    with_app(what, |app| f(app, &sid, &path))
}

/// The methods of a `CodeDocument`.
fn document(lua: &Lua) -> mlua::Result<Table> {
    let m = lua.create_table()?;
    m.raw_set(
        "Version",
        lua.create_function(|_, this: Table| read_doc(&this, "CodeDocument:Version", |app, sid, path| Ok((doc_version(app, sid, path) >> 11) as f64)))?,
    )?;
    m.raw_set(
        "Missing",
        lua.create_function(|_, this: Table| {
            let (sid, path): (String, String) = (this.raw_get("sessionId")?, this.raw_get("Path")?);
            cx::read(Dep::FilesMissing(sid.clone(), path.clone()));
            with_app("CodeDocument:Missing", |app| {
                let doc = app.kernel.slots.get(&crate::files::FILES).sessions.get(&sid).and_then(|w| w.docs.get(&path));
                Ok(doc.is_some_and(|d| d.content().is_missing()))
            })
        })?,
    )?;
    m.raw_set(
        "Binary",
        lua.create_function(|_, this: Table| {
            read_doc(&this, "CodeDocument:Binary", |app, sid, path| {
                let doc = app.kernel.slots.get(&crate::files::FILES).sessions.get(sid).and_then(|w| w.docs.get(path));
                Ok(doc.is_some_and(|d| d.content().is_binary()))
            })
        })?,
    )?;
    m.raw_set(
        "Reload",
        lua.create_function(|_, this: Table| {
            let (sid, path): (String, String) = (this.raw_get("sessionId")?, this.raw_get("Path")?);
            // Read again in place, as a change on disk reads it: the view keeps where it is.
            with_app("CodeDocument:Reload", |app| {
                app.later(Box::new(move |s: &mut App, tx: &mut Tx| {
                    tx.instant();
                    if s.kernel.slots.get(&crate::files::FILES).sessions.get(&sid).is_some_and(|w| w.docs.contains_key(&path)) {
                        crate::files::watch::reload(s, &sid, &path);
                    } else {
                        s.ensure_file_loaded(&sid, &path);
                    }
                }));
                Ok(())
            })
        })?,
    )?;
    method(lua, &m, "Close", |_, _: mlua::MultiValue| Ok(()))?;
    Ok(m)
}

/// The session a tree of `directory` reads: the routed one when it's there (or no folder is
/// given), else the first session open in it.
fn session_for(app: &App, directory: &str) -> Option<String> {
    let page = app.active_session_id();
    let routed = page.and_then(|id| app.session(id));
    if let Some(s) = routed.filter(|s| directory.is_empty() || s.directory == directory) {
        return Some(s.id.clone());
    }
    // The page's session before the server has listed it (a launch on a subagent's page, a slow or
    // absent server): its panel is drawn with no folder yet. The tree is that session's, and
    // lists once its folder is known.
    if routed.is_none() && directory.is_empty() {
        return page.map(str::to_owned);
    }
    app.tabs.iter().filter_map(|t| app.session(&t.id)).find(|s| s.directory == directory).map(|s| s.id.clone())
}

/// The version of what `Dep::Files(session)` reads: the tree, the finds and every open file's
/// state, with where its code is scrolled (a scroll asks for the rows it brings into view).
pub fn version(app: &App, session: &str) -> u64 {
    let mut h = std::hash::DefaultHasher::new();
    if let Some(w) = app.kernel.slots.get(&crate::files::FILES).sessions.get(session) {
        (w.tree_load.is_ready(), w.tree.visible_len(), w.known_files.len(), w.known_dirs.len(), w.expanded_dirs.len(), &w.search, &w.highlighted_filter_path).hash(&mut h);
        let mut docs: Vec<_> = w.docs.iter().collect();
        docs.sort_by(|a, b| a.0.cmp(b.0));
        for (path, d) in docs {
            (path, d.rev, d.content(), d.wrap(), d.line_count(), d.find_open(), d.match_count(), d.active_match_index(), d.text().len()).hash(&mut h);
            (&d.find_input.text, d.find_input.focused).hash(&mut h);
            if d.find_open() { w.file_matches(path,d).hash(&mut h); }
            w.md.get(path).map(|v| (v.mode, v.matches.len(), v.active)).hash(&mut h);
            w.code_paths.get(path).map(|c| c.rev).hash(&mut h);
        }
        // A picture's decode, its zoom and pan, and the stage it fits.
        let mut images: Vec<_> = w.images.iter().collect();
        images.sort_by(|a, b| a.0.cmp(b.0));
        for (path, img) in images {
            let shown = img.shown();
            (path, shown.id(), shown.is_ready(), shown.size(), img.shows(), format!("{:?}", img.zoom)).hash(&mut h);
        }
        // A video's state and controls, and its time at Chromium's `timeupdate` step (0.25 s).
        let mut videos: Vec<_> = w.videos.iter().collect();
        videos.sort_by(|a, b| a.0.cmp(b.0));
        for (path, v) in videos {
            path.hash(&mut h);
            v.version_hash(&mut h);
        }
        app.kernel.slots.get(&crate::files::FILES).stage_size.get().map(f32::to_bits).hash(&mut h);
    }
    h.finish()
}

/// Whether `path` in `session` is ready and missing (`Dep::FilesMissing`).
pub fn missing_version(app: &App, session: &str, path: &str) -> Option<(bool, bool)> {
    app.kernel.slots.get(&crate::files::FILES).sessions.get(session).and_then(|w| w.docs.get(path)).map(|d| (!d.content().is_loading(), d.content().is_missing()))
}

/// The workspace tree and file search version for `session` (`Dep::FilesTree`).
pub fn tree_version(app: &App, session: &str) -> u64 {
    let mut h = std::hash::DefaultHasher::new();
    if let Some(w) = app.kernel.slots.get(&crate::files::FILES).sessions.get(session) {
        (w.tree_load.is_ready(), w.tree.visible_len(), w.known_files.len(), w.known_dirs.len(), w.expanded_dirs.len(), &w.search, &w.highlighted_filter_path).hash(&mut h);
        let mut expanded_xor = 0u64;
        for dir in &w.expanded_dirs {
            let mut dh = std::hash::DefaultHasher::new();
            dir.hash(&mut dh);
            expanded_xor ^= dh.finish();
        }
        expanded_xor.hash(&mut h);
    }
    h.finish()
}

/// One open file's version (`Dep::FilesDoc`): its text, find bar, scroll, markdown and media.
pub fn doc_version(app: &App, session: &str, path: &str) -> u64 {
    let mut h = std::hash::DefaultHasher::new();
    if let Some(w) = app.kernel.slots.get(&crate::files::FILES).sessions.get(session) {
        if let Some(d) = w.docs.get(path) {
            (d.rev, d.content(), d.wrap(), d.line_count(), d.find_open(), d.match_count(), d.active_match_index(), d.text().len()).hash(&mut h);
            (&d.find_input.text, d.find_input.focused).hash(&mut h);
            if d.find_open() { w.file_matches(path, d).hash(&mut h); }
        }
        w.md.get(path).map(|v| (v.mode, v.matches.len(), v.active)).hash(&mut h);
        w.code_paths.get(path).map(|c| c.rev).hash(&mut h);
        if let Some(img) = w.images.get(path) {
            let shown = img.shown();
            (shown.id(), shown.is_ready(), shown.size(), img.shows(), format!("{:?}", img.zoom)).hash(&mut h);
        }
        if let Some(v) = w.videos.get(path) {
            v.version_hash(&mut h);
        }
        if w.images.contains_key(path) || w.videos.contains_key(path) {
            app.kernel.slots.get(&crate::files::FILES).stage_size.get().map(f32::to_bits).hash(&mut h);
        }
    }
    h.finish()
}

/// `ui.codeView`: the open file, its find bar and its code (or a Markdown file's rendered page).
pub fn view(a: &mut NodeArgs) -> Result<El, String> {
    cx::with_app(|app| {
        let pal = p(a);
        let sid = handle_field(a, "doc", "sessionId").ok_or("ui.codeView needs a document from Workspace:OpenDocument")?;
        let path = handle_field(a, "doc", "Path").unwrap_or_default();
        cx::read(Dep::FilesDoc(sid.clone(), path.clone()));
        let empty = crate::files::SessionFiles::default();
        let ws = app.kernel.slots.get(&crate::files::FILES).sessions.get(&sid).unwrap_or(&empty);
        let notes: std::rc::Rc<dyn crate::code_view::notes::LineNotes<App>> =
            crate::line_comments::host::Notes::new(&sid, crate::line_comments::Surface::Files);
        Ok(col().w_full().grow(1.0).min_h(0.0).child(crate::files::viewer::code_area(app, &pal, &sid, ws, &path, true, Some(notes))))
    })
    .ok_or_else(|| "ui.codeView draws only in a render".to_owned())?
}

/// Lands what the file engine read.
pub fn poll(app: &mut App, tx: &mut Tx) -> bool {
    crate::files::poll_files(app, tx)
}

/// The open file's keys.
pub fn key(app: &mut App, ev: &KeyEvent, tx: &mut Tx) -> bool {
    crate::files::key_files(app, ev, tx)
}
