//! The open file: its toolbar, find bar, and either the shared [`CodeViewer`] or, for Markdown,
//! HTML, CSV/TSV tables, Mermaid diagrams, images, and videos, its rendered preview.

mod toolbar;

use std::rc::Rc;

use opencode_code_view::find_bar::{FindBar, find_bar, over};
use opencode_code_view::notes::LineNotes;
use opencode_code_view::{CodeDocState, CodeViewer};
use opencode_ui::keymap::Bind;
use opencode_ui::panel_chrome::{BAR_H, TOGGLE_INSET};
use opencode_ui::popup::PopupHost;
use rune::{Anim, El, Icon, TextEdit, TextStyle, Tx, col, el, icon, row, text};
use rune_theme_opencode::icons as ic;
use opencode_ui::palette::Pal;
use opencode_ui::style::{UI, UI_MED, UI_SMALL};

use crate::markdown::{self, READ_ICON};
use crate::{FilesHost, FilesPane, SessionFiles, html, image, label_find, links, table_view, trace, video};
use toolbar::{META_MIN_W, artifact_toolbar, open_in_button, picture_meta, tool};

const INSTANT: Anim = Anim::INSTANT;
const MONO_TINY: TextStyle = TextStyle::new(11.0, 450.0).mono().lh(15.0).features("'tnum' 1");

static WRAP_ICON: Icon = Icon {
    name: "files-wrap-lines",
    view: 16.0,
    body: r#"<path d="M2.5 4.5H13.5M2.5 8.5H11C12.38 8.5 13.5 9.62 13.5 11C13.5 12.38 12.38 13.5 11 13.5H8.5M10 11.75L8.25 13.5L10 15.25M2.5 12.5H6" stroke="currentColor" stroke-width="1.25" stroke-linecap="round" stroke-linejoin="round" fill="none"/>"#,
};

/// Desktop's `file-tree` icon: a folder with a pane inside.
static PLACEHOLDER_ICON: Icon = Icon {
    name: "files-placeholder-tree",
    view: 20.0,
    body: r#"<path d="M18 18V5H9.5L7.5 2H2L2 18H5M18 18H5M18 18V8.5H5V18" stroke="currentColor" stroke-linecap="square" fill="none"/>"#,
};

fn toggle_file_find(host: &mut dyn FilesHost, sid: &str, path: &str, tx: &mut Tx) {
    tx.instant();
    if let Some(d) = host.files_mut().sessions.get_mut(sid).and_then(|w| w.docs.get_mut(path)) {
        let open = !d.find_open();
        d.set_find_open(open);
    }
}

fn toggle_file_wrap(host: &mut dyn FilesHost, sid: &str, path: &str, tx: &mut Tx) {
    tx.instant();
    let wrap = host
        .files()
        .sessions
        .get(sid)
        .and_then(|w| w.docs.get(path))
        .is_some_and(|d| d.wrap());
    markdown::set_file_wrap(host, sid, path, !wrap);
}

#[allow(clippy::too_many_arguments)]
pub fn files_main<H: FilesHost + PopupHost + 'static>(
    s: &H,
    p: &Pal,
    session_id: &str,
    proj_title: &str,
    ws: &SessionFiles,
    active_path: Option<&str>,
    placeholder: bool,
    sidebar_open: bool,
    notes: Option<Rc<dyn LineNotes<H>>>,
) -> El {
    let Some(path) = active_path.filter(|_| !placeholder) else {
        // `SessionFilePanelV2Empty` at the panel's top: the 20 px `file-tree` icon 8 px above a
        // 13/500 title and the 13/440 hint, 8 px apart, in `text-weak`.
        return col()
            .key("files-placeholder")
            .grow(1.0)
            .min_w(0.0)
            .h_full()
            .padding(24.0)
            .items_center()
            .child(icon(&PLACEHOLDER_ICON, 20.0).color(p.muted))
            .child(el().h(16.0))
            .child(text(opencode_i18n::t!("plugin.file.command.open")).font(UI_MED).color(p.text))
            .child(el().h(8.0))
            .child(el().h(20.0).center().child(text(opencode_i18n::t!("plugin.file.selectToOpen")).font(UI).color(p.muted)));
    };

    let doc = ws.docs.get(path);
    if let Some(d) = doc {
        ws.diagram.prepare(session_id, path, d, p);
    }
    let rendered = markdown::shows_rendered(ws, path);
    let previewable = ws.previewable(path);
    let picture = image::showing(ws, path);
    // A picture or a video shows on its stage, with no lines to find or wrap.
    let lines = picture.is_none() && !ws.videos.contains_key(path);
    let wrap_on = doc.is_some_and(|d| d.wrap());
    let find_on = doc.is_some_and(|d| d.find_open());
    let sel_badge = doc.filter(|_| !rendered).and_then(|d| d.selection()).map(|sel| {
        let (a, b) = sel.ordered();
        if a == b { format!("L{a}") } else { format!("L{a}–{b}") }
    });

    let (dir_part, base_name) = opencode_kit::path::split(path);
    let dir_part = Some(dir_part.to_owned()).filter(|d| !d.is_empty());
    let mut crumb = row().key("files-crumb").grow(1.0).shrink().min_w(0.0).gap(3.0).center_y();
    if !sidebar_open && dir_part.is_none() {
        crumb = crumb.child(
            text(format!("{proj_title}/"))
                .font(UI_SMALL)
                .color(p.faint)
                .shrink()
                .min_w(0.0)
                .truncate_fade(),
        );
    }
    if let Some(d) = dir_part {
        crumb = crumb.child(text(d).font(UI_SMALL).color(p.muted).shrink().min_w(0.0).truncate_fade());
    }
    crumb = crumb.child(
        text(base_name.to_owned())
            .font(UI_MED)
            .color(p.text)
            .shrink()
            .min_w(0.0)
            .truncate_fade(),
    );
    if let Some(lbl) = sel_badge {
        crumb = crumb.child(el().px(5.0).py(1.0).radius(4.0).fill(p.layer1).child(text(lbl).font(MONO_TINY).color(p.muted)));
    }

    let at = |f: fn(&mut dyn FilesHost, &str, &str, &mut Tx)| {
        let (sid, path) = (session_id.to_owned(), path.to_owned());
        move |s: &mut H, tx: &mut Tx| f(s, &sid, &path, tx)
    };
    let path_copy = path.to_owned();

    let toolbar = row()
        .key("files-toolbar")
        .w_full()
        .h(BAR_H)
        // The tree toggle floats over the panel's corner; a closed tree's toolbar starts past it.
        .pl(if sidebar_open { 12.0 } else { TOGGLE_INSET })
        .pr(12.0)
        .gap(6.0)
        .center_y()
        .fill(p.bg_base)
        .hover_group_focus()
        .hover_motion(INSTANT, INSTANT)
        .child(crumb)
        .children(
            picture
                .filter(|_| s.files().stage_size.get()[0] > META_MIN_W)
                .map(|img| picture_meta(p, img.meta())),
        )
        .children(s.opens_as_page(path).then(|| {
            let (sid, at) = (session_id.to_owned(), path.to_owned());
            let label = opencode_i18n::t!("plugin.file.view.openInBrowser");
            tool::<H>(p, "files-open-browser-btn", &ic::GLOBE, false, label, label, move |s: &mut H, tx: &mut Tx| {
                tx.instant();
                s.open_workspace_preview(&sid, &at, tx);
            })
        }))
        .children(previewable.then(|| {
            let (glyph, label) = if rendered {
                (&ic::CODE_LINES, opencode_i18n::t!("plugin.file.view.viewSource"))
            } else {
                (&READ_ICON, opencode_i18n::t!("plugin.file.view.viewPreview"))
            };
            tool::<H>(p, "files-render-btn", glyph, false, label, label, at(markdown::toggle_rendered))
        }))
        .children(lines.then(|| {
            tool::<H>(
                p,
                "files-find-btn",
                &ic::MAGNIFYING_GLASS,
                find_on,
                opencode_i18n::t!("rune.files.find"),
                opencode_i18n::t!("rune.files.find"),
                at(toggle_file_find),
            )
            .help_keys(Bind::Find.keycaps())
        }))
        .children((!rendered && lines).then(|| {
            tool::<H>(
                p,
                "files-wrap-btn",
                &WRAP_ICON,
                wrap_on,
                opencode_i18n::t!("plugin.review.settings.wrapLines.title"),
                opencode_i18n::t!("plugin.review.settings.wrapLines.title"),
                at(toggle_file_wrap),
            )
        }))
        .child(tool::<H>(
            p,
            "files-copy-path",
            &ic::COPY,
            false,
            opencode_i18n::t!("plugin.file.open.copyPath"),
            opencode_i18n::t!("plugin.file.open.copyPath"),
            move |_s: &mut H, tx: &mut Tx| {
                tx.copy(path_copy.clone());
            },
        ))
        .children(open_in_button(s, p, "files-open-in", path));

    let out = col()
        .key("files-main")
        .grow(1.0)
        .min_w(0.0)
        .h_full()
        .child(toolbar)
        .child(el().w_full().h(0.5).fill(p.border_muted));
    out.child(code_area(s, p, session_id, ws, path, false, notes))
}

/// Alias for [`code_area`]: renders the open file view with optional line comments `notes`.
pub fn view<H: FilesHost + PopupHost + 'static>(
    s: &H,
    p: &Pal,
    session_id: &str,
    ws: &SessionFiles,
    path: &str,
    artifact: bool,
    notes: Option<Rc<dyn LineNotes<H>>>,
) -> El {
    code_area(s, p, session_id, ws, path, artifact, notes)
}

/// The open file under the panel's own header (`ui.codeView`): its find bar while open, then a
/// plugin's viewer, the rendered Markdown or the code. `artifact` puts a Markdown file's
/// source / preview toggle above it, as Desktop's artifact toolbar does.
pub fn code_area<H: FilesHost + PopupHost + 'static>(
    s: &H,
    p: &Pal,
    session_id: &str,
    ws: &SessionFiles,
    path: &str,
    artifact: bool,
    notes: Option<Rc<dyn LineNotes<H>>>,
) -> El {
    let doc = ws.docs.get(path);
    if let Some(d) = doc {
        ws.diagram.prepare(session_id, path, d, p);
    }
    let rendered = markdown::shows_rendered(ws, path);
    let previewable = ws.previewable(path);
    // A picture shows on its stage, which has no lines to find or wrap.
    let picture = image::showing(ws, path);
    let clip = ws.videos.get(path);
    let mut out = col().key("files-code-area").grow(1.0).min_w(0.0).min_h(0.0).w_full();
    // A plugin's file viewer (`FileViewer`) shows the kinds of file it takes.
    let plugin_view = s.plugin_file_viewer(p, session_id, &links::link_directory(s, session_id), path);
    // `artifact-view-machine.ts` at 9ca3daec38: ordinary text has fixed source mode.
    // It shares the artifact toolbar's content actions with switchable previews.
    let loaded = doc.is_some_and(|d| d.content().has_content());
    let toolbar = artifact && (loaded || picture.is_some() || clip.is_some() || plugin_view.is_some());
    if toolbar {
        let source = previewable
            || (picture.is_none()
                && clip.is_none()
                && plugin_view.is_none()
                && doc.is_some_and(|d| d.content().is_text()));
        let meta = picture
            .filter(|_| s.files().stage_size.get()[0] > META_MIN_W)
            .map(|img| img.meta());
        let menu_open = s.popup().open_at("files-artifact-menu");
        let tb_hash = {
            use std::hash::{Hash, Hasher};
            let mut h = std::hash::DefaultHasher::new();
            (
                (p.theme, p.generation, p.dark),
                opencode_i18n::locale().id(),
                session_id,
                path,
                rendered,
                previewable,
                source,
                menu_open,
                &meta,
            )
                .hash(&mut h);
            h.finish()
        };
        out = out.child(rune::memo("files-artifact-toolbar", tb_hash, || {
            artifact_toolbar::<H>(p, session_id, path, rendered, previewable, source, menu_open, meta)
        }));
    }
    // Pierre's bar floats over the view it searches, rendered or source.
    let bar = doc.filter(|d| d.find_open()).map(|d| {
        let (active, total) = ws.file_matches(path, d);
        let bar = FindBar {
            key: "files-find-bar",
            top: -4.0,
            edit: file_find_input::<H>,
            changed: find_changed::<H>,
            step: find_step::<H>,
            close: find_close::<H>,
            target: format!("{session_id}\n{path}"),
        };
        find_bar(p, &d.find_input, total, active, s.spellcheck(), bar)
    });

    if let Some(view) = plugin_view {
        return out.child(view);
    }
    if let Some(page) = ws.shown_html(path) {
        let find = if doc.is_some_and(|d| d.find_open()) {
            &ws.html_find
        } else {
            &label_find::LabelFind::Closed
        };
        return out.child(over(html::view(session_id, path, page, find.painter(p)), bar));
    }
    if let Some(table) = ws.shown_table(path) {
        let painter = if doc.is_some_and(|d| d.find_open()) {
            &ws.table_find
        } else {
            &label_find::TableFind::Closed
        };
        return out.child(over(
            table_view::view::<H>(session_id, path, p, table.clone(), painter.painter(p)),
            bar,
        ));
    }
    if let Some(drawing) = ws.shown_diagram(path) {
        let find = if doc.is_some_and(|d| d.find_open()) {
            &ws.diagram_find
        } else {
            &label_find::DiagramFind::Closed
        };
        let marks = find.painter(p);
        return out.child(over(
            drawing.view::<H>(session_id, path, p, |id| marks(&drawing, id)),
            bar,
        ));
    }
    if let Some(img) = picture {
        return out.child(image::stage(s, p, img));
    }
    // One that can't play shows as another binary file, as Desktop's `decode.failed` does.
    if let Some(v) = clip.filter(|v| !matches!(v.player.state(), rune::PlayerState::Failed(_))) {
        return out.child(video::stage(s, p, session_id, path, v));
    }
    if rendered && markdown::is_markdown(path) && let Some(d) = doc {
        return out.child(over(
            markdown::rendered::<H>(p, session_id, path, d, ws.md.get(path), ws.code_paths.get(path), artifact),
            bar,
        ));
    }

    let (sid_get, path_get) = (session_id.to_owned(), path.to_owned());
    let (sid_mut, path_mut) = (session_id.to_owned(), path.to_owned());
    let mut viewer = CodeViewer::new(
        format!("file-code-{session_id}-{path}"),
        move |s: &H| doc_ref(s, &sid_get, &path_get),
        move |s: &mut H| doc_mut(s, &sid_mut, &path_mut),
    )
    .trace(trace::hooks());
    if let Some(notes) = notes {
        viewer = viewer.notes(notes);
    }
    // An artifact view draws its code on `bg-layer-01`, under the rows and past the last
    // (`--opencode-diffs-bg`, `gui-plugins/src/file/artifact-view.css:3-7`). Plain text and code,
    // which have no preview to switch to, stay on the code view's own ground with their toolbar
    // (`[data-source-only]`, opencode#65976).
    let code_pal = match (artifact, previewable) {
        (true, true) => Pal { code_bg: p.layer1, bg_base: p.layer1, ..*p },
        // Under the rows and past the last too, not the panel's base.
        (true, false) => Pal { bg_base: p.code_bg, ..*p },
        (false, _) => *p,
    };

    out.child(over(viewer.build(s, &code_pal), bar))
}

fn doc_ref<'a>(host: &'a dyn FilesHost, sid: &str, path: &str) -> &'a CodeDocState {
    let files = host.files();
    files
        .sessions
        .get(sid)
        .and_then(|w| w.docs.get(path))
        .unwrap_or(&files.empty_doc)
}

fn doc_mut<'a>(host: &'a mut dyn FilesHost, sid: &str, path: &str) -> &'a mut CodeDocState {
    let FilesPane { sessions, empty_doc, .. } = host.files_mut();
    sessions.get_mut(sid).and_then(|w| w.docs.get_mut(path)).unwrap_or(empty_doc)
}

/// The find field of file `at` (`<session>\n<path>`).
fn file_find_input_dyn<'a>(host: &'a mut dyn FilesHost, at: &str) -> &'a mut TextEdit {
    let (sid, path) = at.split_once('\n').unwrap_or_default();
    let FilesPane { sessions, empty_doc, .. } = host.files_mut();
    match sessions.get_mut(sid).and_then(|w| w.docs.get_mut(path)) {
        Some(doc) => &mut doc.find_input,
        None => &mut empty_doc.find_input,
    }
}

fn file_find_input<'a, H: FilesHost>(s: &'a mut H, at: &str) -> &'a mut TextEdit {
    file_find_input_dyn(s, at)
}

fn on_find_changed(host: &mut dyn FilesHost, at: &str, tx: &mut Tx) {
    tx.instant();
    let (sid, path) = at.split_once('\n').unwrap_or_default();
    if let Some(w) = host.files_mut().sessions.get_mut(sid)
        && w.shown_html(path).is_none()
        && w.shown_table(path).is_none()
        && w.shown_diagram(path).is_none()
        && let Some(d) = w.docs.get_mut(path)
    {
        d.sync_find_query();
    }
    markdown::sync_rendered_find(host, sid, path, tx);
}

fn find_changed<H: FilesHost>(s: &mut H, at: &str, tx: &mut Tx) {
    on_find_changed(s, at, tx);
}

fn on_find_step(host: &mut dyn FilesHost, at: &str, forward: bool, tx: &mut Tx) {
    let (sid, path) = at.split_once('\n').unwrap_or_default();
    markdown::file_find_step(host, sid, path, forward, tx);
}

fn find_step<H: FilesHost>(s: &mut H, at: &str, forward: bool, tx: &mut Tx) {
    on_find_step(s, at, forward, tx);
}

fn on_find_close(host: &mut dyn FilesHost, at: &str, tx: &mut Tx) {
    tx.instant();
    let (sid, path) = at.split_once('\n').unwrap_or_default();
    if let Some(d) = host.files_mut().sessions.get_mut(sid).and_then(|w| w.docs.get_mut(path)) {
        d.set_find_open(false);
    }
}

fn find_close<H: FilesHost>(s: &mut H, at: &str, tx: &mut Tx) {
    on_find_close(s, at, tx);
}
