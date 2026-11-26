//! Shared OpenCode session composer (`p254`): Brocode's auto-growing editor (`180pt` cap
//! with internal edge-faded scroll), attachment chips & image paste, prompt history (`history`),
//! `+` attach button, and instant Send / Stop button with `stop_glyph_swap()`.

pub mod attachments;
pub use opencode_runtime::composer_mode as mode;
pub use opencode_ui::prompt_history as history;

use std::cell::Cell;

use opencode_kit::client::PathAttachment;
use opencode_kit::model::Attachment;
use rune::{
    Color, El, KeyEvent, MotionIntent, Shadow, Spring, SpringPair, TextEdit, TextStyle,
    Transition, Tx, col, el, icon, row, spacer, text,
};
use rune_theme_opencode::icons as ic;

use crate::TimelineHost;
use crate::focus::FocusExt;
use crate::palette::{Elevate as _, Pal};
use crate::style::{DOCK_SIDE, UI};
use crate::user_parts::mention_pill_style;

/// The editor's height cap, its padding included (Desktop's `max-h-[180px]`).
pub const COMPOSER_MAX_H: f32 = 180.0;
pub const PROMPT: TextStyle = TextStyle::new(13.0, 440.0).lh(20.0);
/// Shell mode's editor (`editor.tsx:322`): the prompt's 13/20 at 440 in the mono face, -0.04 tracking.
pub const SHELL_PROMPT: TextStyle = TextStyle::new(13.0, 440.0).mono().lh(20.0).tracking(-0.04);

thread_local! {
    static WHOLE: Cell<bool> = const { Cell::new(true) };
}

/// Whether the composer showed its whole prompt when last laid out: a longer one scrolls inside
/// the editor, its words starting out of sight.
pub fn shows_whole_prompt() -> bool {
    WHOLE.get()
}

/// Where sent prompt `local` shows up. When the timeline shows its end, it rises out of the
/// composer, its words where they were typed. Read further up, nothing flies: the timeline cuts
/// to its end, where the message already stands. A prompt longer than the composer, whose words
/// start out of sight in it, arrives where it lands instead of flying.
pub fn land<S: TimelineHost>(s: &mut S, session: &str, local: &str, from: &str, tx: &mut Tx) {
    if s.ends().at_end(session) {
        if shows_whole_prompt() {
            tx.launch(from, format!("u-{local}/bubble"));
        }
    } else {
        // Re-attach follow immediately alongside the instant jump so a same-tick settle cannot
        // snap back to the old scrolled-up anchor (`interaction.ts:183-199`).
        s.ends_mut().set_at_end(session, true);
        tx.scroll_to_bottom_instant(format!("scroll-{session}"));
    }
}

/// Like [`land`], but keeps a scrolled-up timeline where it is instead of jumping to the end.
pub fn land_in_place<S: TimelineHost>(s: &mut S, session: &str, local: &str, from: &str, tx: &mut Tx) {
    if s.ends().at_end(session) && shows_whole_prompt() {
        tx.launch(from, format!("u-{local}/bubble"));
    }
}

/// The composer box's corner radius: Desktop's form is `rounded-xl`, 10 px (`editor.tsx:254`).
/// The new-session Easter egg's paddle grows out of the composer from this radius.
pub use opencode_ui::style::COMPOSER_RADIUS as RADIUS;

/// Renders the shared composer in its bottom dock (`12pt` side and bottom inset, matching
/// `jump-lane` so the timeline runs right down to its top edge).
pub fn dock<S: ComposerHost>(s: &S, p: &Pal) -> El {
    col()
        .key("dock")
        .w_full()
        .center()
        .px(DOCK_SIDE)
        .pb(DOCK_SIDE)
        .child(composer(s, p))
}

pub use opencode_ui::style::composer_fill;

/// The two glyphs of the submit button changing places: the leaving one shrinks away as the
/// arriving one grows in, quickly and without bounce.
pub fn stop_glyph_swap() -> Transition {
    Transition::scale(0.5)
        .allow_small_scale()
        .springs(SpringPair::new(Spring::new(0.2, 0.0), Spring::new(0.14, 0.0)))
}

/// Whether `ev` is one of `prompt.stop`'s keys (Esc and Ctrl+G unless Settings changed them),
/// which stop a running session in the composer.
pub fn is_stop_shortcut(ev: &KeyEvent) -> bool {
    crate::keymap::Bind::Stop.matches(ev)
}

/// Ghost toolbar button shared by Desktop's agent/model/effort pickers.
pub fn ghost(p: &Pal, key: &str, label: &str, chevron: bool, color: Color) -> El {
    ghost_with(p, key, text(label.to_owned()).font(UI).color(color).truncate_fade(), chevron, false)
}

/// [`ghost`] around any label: `button-v2` ghost-muted at size normal (`button.css:41-44,
/// 247-283`), 28 tall with 6 pt corners and 11 in from its start. The chevron
/// (`editor.tsx:1108`, `-ms-0.5 -me-1` on the 6 pt gap) sits 4 after the label and 7 in
/// from the end, in `icon-icon-muted`, or `icon-icon-base` while its menu is `open`. Its
/// keyboard ring sits 2 inside it, where the toolbar's clip can't cut it (`editor.css:70-73`).
pub fn ghost_with(p: &Pal, key: &str, label: El, chevron: bool, open: bool) -> El {
    row()
        .key(key)
        .role("button")
        .h(28.0)
        .pl(11.0)
        .pr(if chevron { 7.0 } else { 11.0 })
        .gap(4.0)
        .center_y()
        .radius(6.0)
        .hover(p.hover)
        .press(p.pressed)
        .focus_inset(p)
        .child(label)
        .children(chevron.then(|| icon(&ic::CHEVRON_DOWN, 16.0).key("chevron").color(if open { p.icon } else { p.icon_muted })))
}

/// The `/` between a provider and its model (`slash.css`): the neighbouring text's size and
/// the page's 440 (`var(--font-weight-normal)` is never set), `icon-icon-faint`, 6 pt of space
/// to its ink on each side.
pub fn slash(p: &Pal, gap: f32) -> El {
    // The margins less the glyph's side bearings (`margin-inline: -0.25px -0.34px`).
    text("/").font(SLASH).color(p.icon_faint).ml((gap - 0.5).max(0.0) - 0.25).mr((gap - 0.5).max(0.0) - 0.34)
}

const SLASH: TextStyle = TextStyle::new(13.0, 440.0).lh(16.0).tracking(-0.04);

/// Default `+` attach button at the start of the toolbar (`⌘U`). The pickers start 4 after it
/// (`ms-1`), the toolbar's 2 pt gap and its own 2.
pub fn attach_button<S: ComposerHost>(p: &Pal) -> El {
    el()
        .key("add")
        .role("button")
        .label_static(opencode_i18n::t!("ui.promptInput.add"))
        .help(format!("{} {}", opencode_i18n::t!("ui.promptInput.add"), crate::keymap::Bind::Attach.label()))
        .size(28.0, 28.0)
        .mr(2.0)
        .radius(6.0)
        .center()
        .hover(p.hover)
        .press(p.pressed)
        .focus_inset(p)
        .on_press_tx(S::composer_pick_attachment)
        .child(icon(&ic::PLUS, 16.0).color(p.icon_muted))
}

/// Per-window composer state (`ViewState::composer`) for hosts that manage multiple sessions by ID.
#[derive(Clone, Debug, Default)]
pub struct ComposerState {
    pub files: std::collections::HashMap<String, Vec<Attachment>>,
    pub paths: std::collections::HashMap<String, Vec<PathAttachment>>,
    pub uploading: std::collections::HashMap<String, Vec<String>>,
    pub history: history::PromptHistory<String>,
    pub stopped: std::collections::HashSet<String>,
}

impl ComposerState {
    pub fn files(&self, session: &str) -> &[Attachment] {
        self.files.get(session).map_or(&[], Vec::as_slice)
    }

    pub fn paths(&self, session: &str) -> &[PathAttachment] {
        self.paths.get(session).map_or(&[], Vec::as_slice)
    }

    pub fn uploading(&self, session: &str) -> &[String] {
        self.uploading.get(session).map_or(&[], Vec::as_slice)
    }

    pub fn add_file(&mut self, session: &str, a: Attachment) {
        attachments::push_unique(self.files.entry(session.to_owned()).or_default(), a);
    }

    pub fn remove_file(&mut self, session: &str, id: &str) {
        if let Some(list) = self.files.get_mut(session) {
            list.retain(|f| crate::images::identity(f) != id);
        }
    }

    pub fn add_path(&mut self, session: &str, p: PathAttachment) {
        let list = self.paths.entry(session.to_owned()).or_default();
        if !list.iter().any(|x| x.path == p.path) {
            list.push(p);
        }
    }

    pub fn remove_path(&mut self, session: &str, path: &str) {
        if let Some(list) = self.paths.get_mut(session) {
            list.retain(|f| f.path != path);
        }
    }

    pub fn take(&mut self, session: &str) -> (Vec<Attachment>, Vec<PathAttachment>) {
        (
            self.files.remove(session).unwrap_or_default(),
            self.paths.remove(session).unwrap_or_default(),
        )
    }

    pub fn restore(&mut self, session: &str, files: Vec<Attachment>, paths: Vec<PathAttachment>) {
        for a in files {
            self.add_file(session, a);
        }
        for p in paths {
            self.add_path(session, p);
        }
    }
}

/// Host hooks for the shared session composer. Defaults give a minimal host auto-grow,
/// attachment chips, image paste, `+` attach button, and Send / Stop with ~20 lines of glue,
/// while letting Brocode plug in its pickers, slash/mention menus, queue face, and dictation.
/// The prompt's name and the suggestion row it names, for a screen reader: built only while one
/// reads the window, so a keystroke without one makes nothing for them.
fn named_prompt<S: ComposerHost>(s: &S, prompt: El) -> El {
    if !s.assistive() {
        return prompt;
    }
    // Nothing highlighted names no row.
    prompt.label_static(opencode_i18n::t!("ui.promptInput.label")).active_descendant(s.composer_active_option().unwrap_or_default())
}
pub trait ComposerHost: TimelineHost {
    fn composer_edit(&self) -> &TextEdit;
    fn composer_edit_mut(&mut self) -> &mut TextEdit;

    fn composer_busy(&self) -> bool {
        self.active_session_id().is_some_and(|id| self.is_session_live(id))
    }
    fn composer_send(&mut self, tx: &mut Tx);
    fn composer_send_in_place(&mut self, tx: &mut Tx) {
        self.composer_send(tx);
    }
    fn composer_shift_held(&self) -> bool {
        false
    }
    fn composer_stop(&mut self, tx: &mut Tx);

    fn composer_attachments(&self) -> &[Attachment] {
        &[]
    }
    fn composer_remove_attachment(&mut self, _identity: &str) {}
    fn composer_paste_attachment(&mut self, _attachment: Attachment) -> bool {
        false
    }
    /// Files pasted from the clipboard (Finder's ⌘C); true when the host attached them.
    fn composer_paste_files(&mut self, _files: &[std::path::PathBuf]) -> bool {
        false
    }
    fn composer_pick_attachment(&mut self, _tx: &mut Tx) {}

    fn composer_path_attachments(&self) -> &[PathAttachment] {
        &[]
    }
    fn composer_remove_path_attachment(&mut self, _path: &str) {}
    /// Uploads still copying, by id, file name and percent sent.
    fn composer_uploading(&self) -> Vec<(String, String, u32)> {
        Vec::new()
    }
    /// The upload card's cancel.
    fn composer_cancel_upload(&mut self, _id: &str) {}
    fn composer_quote_chips(&self, _p: &Pal) -> Vec<El> {
        Vec::new()
    }
    fn composer_has_extra_payload(&self) -> bool {
        false
    }

    /// The key of the suggestion row the composer's popover highlights (`@`, `/`), which the
    /// prompt names as its active descendant while focus stays in it.
    fn composer_active_option(&self) -> Option<String> {
        None
    }
    fn composer_placeholder(&self) -> String {
        opencode_i18n::t!("ui.promptInput.placeholder.normal", slash = "/", at = "@")
    }
    fn composer_shell_mode(&self) -> bool {
        false
    }
    /// The host renders the touch actions below the window, independently of the editor.
    fn composer_docked_actions(&self) -> bool { false }
    /// The app's spell checking setting (Desktop's `spellcheck`): the prompt marks misspelled
    /// words while it is on, outside shell mode (`editor.tsx:318`).
    fn composer_spellcheck(&self) -> bool {
        false
    }
    fn composer_dictating(&self) -> bool {
        false
    }
    /// The composer sits on docks above it (`data-dock-border-underlay`, a session's): a 0.5 pt
    /// ring of the page's colour under its elevation masks the tray edges it overlaps.
    fn composer_border_underlay(&self) -> bool {
        false
    }
    /// A queued prompt is open in the composer for an edit: Return confirms it, never stops.
    fn composer_editing(&self) -> bool {
        false
    }
    fn composer_face(&self, _p: &Pal) -> Option<Vec<El>> {
        None
    }
    fn composer_face_transition(&self) -> Transition {
        Transition::none()
    }
    /// Whether the face draws its own surfaces (the Classic question's raised body over its
    /// footer tray), so the composer's card steps back while it shows.
    fn composer_face_bare(&self) -> bool {
        false
    }
    fn composer_toolbar_items(&self, p: &Pal, _scope: &str, _queues: bool) -> Vec<El> {
        vec![
            attach_button::<Self>(p),
            spacer().key("room").h(28.0).min_w(0.0),
        ]
    }
    fn composer_below_toolbar(&self, _p: &Pal) -> Option<El> {
        None
    }
    /// The height the composer holds while previews of other sizes replace its draft (Desktop's
    /// history list keeps the height it opened over, `editor.tsx:249`).
    fn composer_held_height(&self) -> Option<f32> {
        None
    }
    fn composer_on_change(&mut self, _tx: &mut Tx) {}
    fn composer_set_hovered(&mut self, _hovered: bool) {}
    fn composer_on_key(&mut self, _ev: &KeyEvent, _tx: &mut Tx) -> bool {
        false
    }
    /// Right-click on an inline mention pill inside the composer (`El::on_pill_secondary`).
    fn composer_on_pill_secondary(&mut self, _id: &str, _press: rune::PointerPress, _tx: &mut Tx) -> bool {
        false
    }
}

/// Whether the composer holds something to send: words (whitespace doesn't count), or files,
/// paths and quotes outside shell mode.
pub fn ready<S: ComposerHost>(s: &S) -> bool {
    let has_extra = !s.composer_attachments().is_empty()
        || !s.composer_path_attachments().is_empty()
        || s.composer_has_extra_payload();
    !s.composer_edit().text.trim().is_empty() || (has_extra && !s.composer_shell_mode())
}

/// Whether Send shows as Stop: the open session is busy and there's nothing to send.
pub fn stopping<S: ComposerHost>(s: &S) -> bool {
    s.composer_busy() && !ready(s) && !s.composer_dictating()
}

/// ↵ (⌘↵ too): sends, or stops the run when Send shows as Stop, as Desktop does. A held ↵'s
/// repeats never stop it.
pub fn submit_or_stop<S: ComposerHost>(s: &mut S, tx: &mut Tx) {
    // A queued prompt open for an edit goes back to the queue, even blank (`model.ts:550-556`).
    if !stopping(s) || s.composer_editing() {
        return s.composer_send(tx);
    }
    if !tx.key_repeat() {
        s.composer_stop(tx);
    }
}

/// Desktop's composer submit (`editor.tsx:1483-1497`): `icon-button-v2` in the `submit`
/// variant at `size-7 rounded-md`, as `submit.css` paints it. `bg-icon-button-contrast` under
/// the top-lit `--submit-highlight` (white at 20%, 60% on dark's light fill), ringed and
/// lifted by `--v2-elevation-button-contrast` (`theme.css:358-361,507-510`); hover and press
/// lay `--submit-hover` and `overlay-contrast-pressed` over it. Without anything to send it
/// is disabled: the same button at 40% (`submit.css:74-80`), with no hover.
pub fn submit_button(p: &Pal, live: bool) -> El {
    submit_surface(el().key("submit").role("button").size(28.0, 28.0).radius(6.0).center(), p, live)
}

/// Button-v2 submit elevation, including the disabled ink, for icon and labelled buttons.
pub fn submit_surface(base: El, p: &Pal, live: bool) -> El {
    let shadow = |y: f32, blur: f32, spread: f32, color: Color, inset: bool| Shadow { y, blur, spread, color, inset, ..Shadow::default() };
    let (white, black) = (Color::WHITE, Color::BLACK);
    let elevation = if p.dark {
        vec![
            shadow(1.0, 2.0, 0.0, black.alpha(0.4), false),
            shadow(0.0, 0.0, 0.5, p.q.submit_ring, false),
            shadow(-0.5, 0.0, 0.0, white.alpha(0.3), false),
        ]
    } else {
        vec![
            shadow(1.0, 1.5, 0.0, black.alpha(0.2), false),
            shadow(0.0, 0.0, 0.5, p.q.submit_ring, false),
            shadow(1.0, 2.0, 0.0, white.alpha(0.14), true),
            shadow(-1.0, 2.0, 0.0, black.alpha(0.06), true),
        ]
    };
    let b = elevation
        .into_iter()
        .fold(base.fill(p.q.submit), El::box_shadow)
        .gradient(white.alpha(if p.dark { 0.6 } else { 0.2 }), white.alpha(0.0))
        .motion(Spring::NONE)
        .motion_intent(MotionIntent::CUT);
    if !live {
        // Disabled, so neither clickable nor a Tab stop (`editor.tsx:1548`), under the not-allowed cursor
        // (`packages/ui/src/actions/submit.css:79`; Desktop's cursor reset leaves `:disabled` alone).
        return b.opacity(0.4).disabled(true).focusable(false).hit().cursor(rune::Cursor::NotAllowed);
    }
    let (hover, pressed) = if p.dark { (black.alpha(0.08), black.alpha(0.4)) } else { (white.alpha(0.12), white.alpha(0.24)) };
    b.hover(hover).press(pressed).focus_default(p)
}

/// Renders the shared composer (`key("composer")`).
/// The room a scroll bar takes from layout where a scroll box reserves it (`scrollbar.css`).
const SCROLLBAR: f32 = 8.0;

/// `editor.tsx:313, 324`: the prompt takes the direction of its text (`dir="auto"`) and each of
/// its lines its own (`unicode-bidi: plaintext`); a shell command runs left to right.
fn prompt_dir(field: El, shell: bool) -> El {
    if shell { field.dir(rune::Direction::Ltr) } else { field.dir(rune::Direction::Auto).dir_by_paragraph() }
}

/// Renders only an active request dock (question or permission) without the prompt composer,
/// used on child/subagent sessions where no composer is shown
/// (`packages/app/src/session/composer/session-composer-region.tsx:66-98`).
pub fn request_dock<S: ComposerHost>(s: &S, p: &Pal) -> Option<El> {
    let face = s.composer_face(p)?;
    let box_el = col()
        .key("composer")
        .role("form")
        .w_full()
        .max_w(crate::style::dock_w())
        .corners([RADIUS; 4]);
    let box_el = if s.composer_face_bare() {
        box_el
    } else {
        let underlay = if s.composer_border_underlay() { box_el.box_shadow(Pal::ring_out(p.bg_base)) } else { box_el };
        underlay.fill(composer_fill(p)).elevate(p.raised).clip_soft(0.0)
    };
    Some(box_el.children(face))
}

pub fn composer<S: ComposerHost>(s: &S, p: &Pal) -> El {
    composer_in(s, p, "")
}

/// Renders the shared composer, keyed `{scope}-composer` when `scope` isn't empty.
pub fn composer_in<S: ComposerHost>(s: &S, p: &Pal, scope: &str) -> El {
    let docked = s.composer_docked_actions() && scope.is_empty();
    let (pad, shown) = (
        rune::memo::layout_with("editor-pad", |r| r[3]),
        rune::memo::layout_with("editor-scroll", |r| r[3]),
    );
    WHOLE.set(!matches!((pad, shown), (Some(pad), Some(shown)) if pad > shown + 0.5));
    let ready = ready(s);
    let busy = s.composer_busy();
    let stop = stopping(s);
    let glyph = match (stop, s.composer_shell_mode()) {
        (true, _) => icon(&ic::STOP, 16.0).key("stop-glyph"),
        (false, true) => icon(&ic::ARROW_UNDO_DOWN, 16.0).key("send-glyph"),
        (false, false) => icon(&ic::ARROW_UP, 16.0).key("send-glyph"),
    };
    // Send finishes a dictation and sends its words, so it stays live while one runs.
    let submit = (!docked).then(|| {
        let live = ready || stop || s.composer_dictating();
        let label = if stop { opencode_i18n::t!("ui.promptInput.stop") } else { opencode_i18n::t!("ui.promptInput.send") };
        let b = submit_button(p, live).label(label).child(glyph.color(p.q.submit_text).transition(stop_glyph_swap()));
        // Official `<Tooltip placement="top" inactive={!stopping && disabled}>` and
        // `tabIndex={mode === "normal" ? undefined : -1}` (`editor.tsx:1546-1569`).
        if live {
            let b = if s.composer_shell_mode() { b.tab_index(-1) } else { b };
            b.help(label).help_above().on_press_tx(move |s: &mut S, tx: &mut Tx| {
                if stop {
                    s.composer_stop(tx);
                } else if s.composer_shift_held() {
                    s.composer_send_in_place(tx);
                } else {
                    s.composer_send(tx);
                }
            })
        } else {
            b
        }
    });
    let key = if scope.is_empty() {
        "composer".to_owned()
    } else {
        format!("{scope}-composer")
    };
    let face = s.composer_face(p);
    // Track pointer hover explicitly rather than `.hover_group()` so prompt keyboard focus does
    // not reveal the default effort button (`editor.css:87`).
    // A landmark F6 lands on (the prompt, or the request standing in its place).
    let box_el = col()
        .key(key)
        // Unnamed, as official's `<form>`: a name would cost the view an allocation each run,
        // and the prompt inside names it as F6 lands.
        .role("form")
        .w_full()
        .max_w(crate::style::dock_w())
        .min_h(if docked && s.composer_shell_mode() { 44.0 } else { 88.0 })
        // `rounded-xl` (`editor.tsx:254`): the app's `--radius-xl` is 0.625rem (`theme.css:50`).
        .corners([RADIUS; 4])
        .on_hover_change(|s: &mut S, inside, tx: &mut Tx| {
            s.composer_set_hovered(inside);
            tx.instant();
        });
    // A face with surfaces of its own keeps its shadows: no card under it and no clip over it.
    // Otherwise `--v2-elevation-raised` (`editor.tsx:248-251`): two drops, a 0.5 pt ring and
    // dark's lit top edge.
    let box_el = if face.is_some() && s.composer_face_bare() {
        box_el
    } else {
        let underlay = if s.composer_border_underlay() { box_el.box_shadow(Pal::ring_out(p.bg_base)) } else { box_el };
        underlay.fill(composer_fill(p)).elevate(p.raised).clip_soft(0.0)
    };
    let held = s.composer_held_height();
    let box_el = match held {
        Some(h) => box_el.h(h),
        None => box_el,
    };
    match face {
        Some(face) => box_el.children(face),
        None => box_el
            // Clicking empty space on the composer card (padding, gutter, toolbar spacer) puts
            // pointer focus in the prompt without a keyboard ring (`field-focus.tsx` `pointer()`),
            // instead of blurring it so typing enters via `type_to_compose` as a stray keyboard focus.
            .preserve_focus_on_press(true)
            .press_on_down()
            .on_press_tx(|s: &mut S, tx: &mut Tx| {
                if !s.composer_edit().focused {
                    let end = s.composer_edit().text.len();
                    let e = s.composer_edit_mut();
                    e.set_selection(end, end);
                    e.focused = true;
                }
                tx.focus("prompt", false);
            })
            // Shell mode sends no attachments, so it shows none (`editor.tsx:272`).
            .children(if s.composer_shell_mode() { None } else { attachments::chips(s, p) })
            .child(
                col()
                    .key("editor")
                    .transition(s.composer_face_transition())
                    .w_full()
                    .shrink()
                    .grow(if held.is_some() { 1.0 } else { 0.0 })
                    // `composer-scroll`: a plain `overflow-y: auto` box's bar (`editor.tsx:296-300`), in
                    // the room `editor-pad` keeps for it. Drop the 180 pt cap while holding height for
                    // history previews (`editor.tsx:297`).
                    .child(crate::scroller::scroller("editor-scroll").bar(crate::scroller::Bar::Styled(crate::scroller::native_bar(p))).build::<S>(
                        p,
                        {
                            let vp = col()
                                .w_full()
                                .shrink()
                                // `editor.css:9-20`: a fixed 16 fade over the top padding, and the
                                // bottom feathering by as much as is left to scroll, up to 16.
                                .clip_fade_linear(0.0, 16.0, 0.0, 0.0)
                                .scroll_fade(rune::ScrollFade::feather(16.0).end_only())
                                .scroll_margin(16.0);
                            if held.is_some() { vp.grow(1.0) } else { vp.max_h(COMPOSER_MAX_H) }
                        },
                                // The scroll box keeps its 8 pt scroll bar's room at its end
                                // (`scrollbar-gutter: stable`, `--scrollbar-size`), on the app's side.
                                col().key("editor-pad").w_full().pr(SCROLLBAR).child(
                                    named_prompt(s, prompt_dir(
                                        crate::style::field(
                                            s.composer_edit(),
                                            if s.composer_shell_mode() { SHELL_PROMPT } else { PROMPT },
                                        ),
                                        s.composer_shell_mode(),
                                    )
                                    .key("prompt"))
                                    // `ps-4 pt-4 pb-2`, and 12 less the gutter at the end
                                    // (`editor.tsx:314`, `editor.css:29-34`), on the editor itself,
                                    // so they follow its own direction and cover the full 44 pt hit box.
                                    .pl(16.0)
                                    .pr(12.0 - SCROLLBAR)
                                    .pt(16.0)
                                    .pb(8.0)
                                    .color(p.text)
                                    .w_full()
                                    .min_h(20.0 + 16.0 + 8.0)
                                    .animate_edit_layout()
                                    .placeholder(s.composer_placeholder(), p.faint)
                                    .caret(p.text, p.selection).selection_style(p.field_selection)
                                    .spellcheck(s.composer_spellcheck() && !s.composer_shell_mode())
                                    .on_edit(S::composer_edit_mut)
                                    .pill_style(mention_pill_style(p))
                                    .on_pill_secondary(S::composer_on_pill_secondary)
                                    .on_input_change(S::composer_on_change)
                                    .on_input_key(S::composer_on_key)
                                    .on_input_paste(attachments::paste::<S>)
                                    .on_submit(submit_or_stop::<S>),
                                ),
                    )),
            )
            .children((!docked || !s.composer_shell_mode()).then(||
                row()
                    .key("toolbar")
                    .transition(s.composer_face_transition())
                    // `editor.tsx:424-462`: 44 tall, 8 in, the pickers 2 apart.
                    .h(44.0)
                    .px(8.0)
                    .gap(2.0)
                    .center_y()
                    .children(s.composer_toolbar_items(p, scope, busy && ready))
                    // The slot rides the composer's edge as a panel's spring moves it, as
                    // Desktop's follows the chat's width; the button's own swaps land at once.
                    .children(submit.map(|b| el().key("submit-slot").size(28.0, 28.0).child(b)))
            ))
            .children(s.composer_below_toolbar(p)),
    }
}
