//! What stands in the composer's place while the open session waits on someone: a question's
//! form (`question`), else a permission (`permission`). The composer's surface morphs into it
//! (`ComposerHost::composer_face`) and back once it's answered, here or anywhere else. Desktop
//! and embedder apps both draw it from here; the host says what's pending and sends the replies.

use opencode_kit::model::Pending;
use rune::{Anim, Color, El, KeyEvent, Shadow, SpringPair, TextStyle, Transition, Tx, row, text};

use crate::composer::ComposerHost;
use crate::focus::FocusExt;
use crate::palette::{Elevate as _, Pal};

pub mod knobs;
pub mod permission;
pub mod question;

pub use question::Dock;

// `button.css:21` sets `font-variant-numeric: tabular-nums` on `button-v2` labels.
pub(crate) const BUTTON: TextStyle = TextStyle::new(13.0, 530.0).lh(20.0).tracking(-0.04).features("\"tnum\"");

/// A host that shows the dock: its open requests and where their replies go.
pub trait DockHost: ComposerHost {
    fn dock(&self) -> &Dock;
    fn dock_mut(&mut self) -> &mut Dock;
    /// The open requests in session `id`'s family (a subagent's show on its root).
    fn dock_pending(&self, id: &str) -> Vec<Pending>;
    /// Session `id`'s parent, when it's a subagent.
    fn dock_parent(&self, id: &str) -> Option<String>;
    /// Whether permissions are answered without asking.
    fn dock_auto_accept(&self) -> bool {
        false
    }
    /// Whether questions answer from the composer (Desktop's Settings › Experimental ›
    /// New question UI, `general.questionComposer`, `opencode#66825`) instead of Classic.
    fn dock_question_composer(&self) -> bool {
        false
    }
    /// The key that moves the question dock on, or submits it on the last question (Desktop's
    /// Settings › General › Submit question answers with, `general.questionSubmitKey`).
    fn dock_question_submit(&self) -> question::SubmitKey {
        question::SubmitKey::Enter
    }
    /// Answers permission `id` with `decision` (`once`, `always`, `reject`).
    fn dock_reply_permission(&mut self, id: &str, decision: &str);
    /// Sends form `id`'s answer (`Dock::answer`).
    fn dock_submit_form(&mut self, id: &str);
    fn dock_dismiss_form(&mut self, id: &str);
    /// Whether request `id` still waits.
    fn dock_waiting(&self, id: &str) -> bool;
    /// Whether request `id`'s reply is on its way or taken (Desktop's `isPermissionResponding`,
    /// `isQuestionBusy`): the dock shows it with its buttons off and takes no answers.
    fn dock_busy(&self, _id: &str) -> bool {
        false
    }
    /// Whether the composer takes the keyboard back when the dock closes.
    fn dock_refocus(&self) -> bool {
        true
    }
    /// A key the typed answer's field doesn't take, for the app's shortcuts.
    fn dock_shortcut(&mut self, _ev: &KeyEvent, _tx: &mut Tx) -> bool {
        false
    }
    /// The height of the pane the dock sits in, which a long question keeps to part of.
    fn dock_pane_h(&self) -> Option<f32> {
        ["story-session", "panel"].into_iter().find_map(|k| rune::memo::layout_with(k, |r| r[3]))
    }
}

/// The composer's face while a request waits: the question's form, else the permission.
pub fn face<S: DockHost>(s: &S, p: &Pal) -> Option<Vec<El>> {
    question::face(s, p).or_else(|| permission::face(s, p))
}

/// Whether the face draws its own surfaces instead of the composer's card: both the Classic and
/// composer-integrated question docks (`opencode#66825`) and the permission dock draw their own cards.
pub fn bare<S: DockHost>(s: &S) -> bool {
    question::id(s).is_some() || permission::asked(s).is_some()
}

pub use opencode_ui::palette::flat;

/// What stands in the composer's place, by id: the question's form, else a permission.
pub fn shown<S: DockHost>(s: &S) -> Option<String> {
    question::id(s).or_else(|| permission::asked(s).map(|p| p.id().to_owned()))
}

/// A dock (a question or a permission) opened or closed: the surface springs to its new face,
/// the timeline pinned above it rides its top edge, and the composer gets the keyboard back.
pub fn morph<S: DockHost>(s: &mut S, tx: &mut Tx) {
    tx.motion_curve(knobs::morph(s.knobs()));
    tx.follow_edge();
    if shown(s).is_none() && s.dock_refocus() {
        s.composer_edit_mut().focused = true;
    }
}

/// How the composer's face and the dock's swap inside the one surface: the face that leaves
/// fades out quickly, the one that arrives rises and fades up with the morph. The morph is
/// shared by opening and closing, so the dock leaves the way it came.
pub fn face_transition(k: &rune_knobs::Knobs) -> Transition {
    Transition::asymmetric(Transition::offset(0.0, k.get(&knobs::FACE_RISE)).combined(Transition::opacity()), Transition::opacity())
        .springs(SpringPair { enter: knobs::spring(k, &knobs::FACE_IN), exit: knobs::spring(k, &knobs::FADE_OUT) })
}

#[derive(Clone, Copy, PartialEq)]
pub enum Variant {
    Ghost,
    /// A ghost button in a question's footer, which mutes it (`question-footer > ghost`).
    GhostMuted,
    Neutral,
    Submit,
    /// `contrast`: the Classic question's Submit, on `bg-contrast` rather than Submit's
    /// `bg-icon-button-contrast` (the two differ in dark).
    Contrast,
}

/// `button-v2` at size large.
pub fn button(p: &Pal, key: &str, label: &str, variant: Variant, hint: Option<&str>, live: bool) -> El {
    let (fill, fg) = match variant {
        Variant::Ghost => (Color::TRANSPARENT, p.text),
        Variant::GhostMuted => (Color::TRANSPARENT, p.muted),
        // Dark's neutral fill is translucent (`alpha-light-6`), and the theme flattens it over
        // `bg-base`; the docks it sits on are `layer-01`, so it's laid over that instead.
        Variant::Neutral if p.dark && p.theme == 0 => (flat(Color::WHITE.alpha(0.06), p.layer1), p.text),
        Variant::Neutral => (p.q.neutral, p.text),
        Variant::Submit => (p.q.submit, p.q.submit_text),
        Variant::Contrast => (p.contrast, p.q.contrast_text),
    };
    let mut b = row()
        .key(key)
        .role("button")
        .label(label.to_owned())
        .h(32.0)
        .px(15.0)
        .gap(6.0)
        .center_y()
        .radius(6.0)
        .fill(fill)
        .hover_motion(Anim::INSTANT, Anim::INSTANT)
        .child(text(label.to_owned()).font(BUTTON).color(fg))
        .children(hint.map(|h| text(h.to_owned()).key("hint").font(BUTTON).color(fg).opacity(0.6)));
    // Disabled, as `button-v2` dims it: 0.4 for Submit and Contrast (`submit.css`), else 0.5.
    let dim = if matches!(variant, Variant::Submit | Variant::Contrast) { 0.4 } else { 0.5 };
    // Submit and Contrast lay the contrast overlays over their dark fill (`submit.css`: dark's
    // Submit `--submit-hover` is `alpha-dark-8`; button.css's `contrast`); the rest the simple ones.
    let (hover, pressed) = match variant {
        Variant::Submit if p.dark => (Color::BLACK.alpha(0x14 as f32 / 255.0), p.v2.contrast_pressed),
        Variant::Submit | Variant::Contrast => (p.v2.contrast_hover, p.v2.contrast_pressed),
        Variant::Ghost | Variant::GhostMuted | Variant::Neutral => (p.hover, p.pressed),
    };
    b = if live { b.focus_default(p).hover(hover).press(pressed) } else { b.opacity(dim).disabled(true).focusable(false).hit().cursor(rune::Cursor::NotAllowed) };
    match variant {
        Variant::Ghost | Variant::GhostMuted => {}
        Variant::Neutral => b = b.elevate(opencode_ui::toast::neutral_lift(p)),
        // `--v2-elevation-button-contrast` under the top-lit highlight (`--submit-highlight`,
        // brighter on dark's light Submit fill).
        Variant::Submit | Variant::Contrast => {
            let lit = if variant == Variant::Submit && p.dark { 0.6 } else { 0.2 };
            let (y, blur, drop) = if p.dark { (1.0, 2.0, 0.4) } else { (1.0, 1.5, 0.2) };
            b = b.gradient(Color::WHITE.alpha(lit), Color::WHITE.alpha(0.0)).box_shadow(Pal::ring_out(p.q.submit_ring)).shadow(y, blur, Color::BLACK.alpha(drop));
            // The rest of the elevation (`theme.css`): light's inner lit top and shaded bottom
            // (`alpha-light-14`, `alpha-dark-6`), dark's lit top edge 0.5 pt up (`alpha-light-30`).
            let inset = |y: f32, color: Color| Shadow { y, blur: 2.0, color, inset: true, ..Shadow::default() };
            b = if p.dark {
                b.box_shadow(Shadow { y: -0.5, color: Color::WHITE.alpha(0x4d as f32 / 255.0), ..Shadow::default() })
            } else {
                b.box_shadow(inset(1.0, Color::WHITE.alpha(0x24 as f32 / 255.0))).box_shadow(inset(-1.0, Color::BLACK.alpha(0x0f as f32 / 255.0)))
            };
        }
    }
    b
}
