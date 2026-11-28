//! The inspector's shared pieces: its type styles, a labelled row, the status dot, a session
//! link, and the card a prompt, code or a run's output sits in.

use opencode_i18n::t;
use opencode_kit::model::{AutomationInfo, AutomationOutcome, AutomationStatus};
use opencode_kit::servers::ServerKey;
use opencode_ui::palette::Pal;
use opencode_ui::widgets::status_ink;
use rune::{Anim, Direction, El, TextStyle, Tx, col, el, icon, row, text};
use rune_theme_opencode::icons as ic;
use session_view::focus::FocusExt as _;

use crate::App;

use super::AUTOMATIONS;
use super::fold::{self, Fold};

pub(super) const TITLE: TextStyle = TextStyle::new(16.0, 600.0).lh(24.0).tracking(-0.16);
pub(super) const BODY: TextStyle = TextStyle::new(13.0, 440.0).lh(20.0).tracking(-0.04);
pub(super) const BODY_MED: TextStyle = TextStyle::new(13.0, 530.0).lh(20.0).tracking(-0.04);
pub(super) const META: TextStyle = TextStyle::new(12.0, 440.0).lh(20.0);
pub(super) const META_MED: TextStyle = TextStyle::new(12.0, 530.0).lh(20.0);
pub(super) const CODE: TextStyle = TextStyle::new(12.0, 440.0).lh(20.0).mono();

/// [`CODE`] on a line of [`BODY`] is a size smaller in the same 20 pt line, so centred its
/// baseline lands half a point over the sans's. Set this much lower, the two share one.
pub(super) const CODE_DROP: f32 = 0.5;

/// Mono text that shares a line with [`BODY`], on its baseline.
pub(super) fn code_inline(words: El) -> El {
    words.font(CODE).mt(CODE_DROP).mb(-CODE_DROP)
}

/// The label column every row of the inspector shares, and the gap to its values.
const LABEL_W: f32 = 88.0;
const LABEL_GAP: f32 = 16.0;

/// A labelled row: the label in the shared column, its values stacked beside it.
pub(super) fn field(p: &Pal, label: &str, font: TextStyle, values: El) -> El {
    row()
        .w_full()
        .gap(LABEL_GAP)
        .items_start()
        .child(text(label).w(LABEL_W).font(font).color(p.faint))
        .child(values.grow(1.0).shrink().min_w(0.0))
}

/// The dot before an automation's status: green while active, a ring while paused, red when a
/// failure paused it, amber when it timed out, faint when it's done.
pub(super) fn status_dot(p: &Pal, info: &AutomationInfo) -> El {
    let dot = el().size(6.0, 6.0).radius(3.0);
    let dot = match (info.status, info.last_outcome) {
        (AutomationStatus::Paused, Some(AutomationOutcome::Failed)) => dot.fill(status_ink(p, "failed")),
        (AutomationStatus::Done, Some(AutomationOutcome::Expired)) => dot.fill(status_ink(p, "needs_auth")),
        (AutomationStatus::Active, _) => dot.fill(status_ink(p, "active")),
        (AutomationStatus::Paused, _) => dot.border(1.0, p.icon_muted),
        (AutomationStatus::Done, _) => dot.fill(p.icon_faint),
    };
    el().size(6.0, 20.0).center().child(dot).a11y_hidden()
}

/// A session to open: its title (or a stand-in when it isn't known) and an arrow. The hover wash
/// bleeds past the text, which stays on its column's edge.
pub(super) fn session_link(p: &Pal, key: &str, server: ServerKey, id: &str, title: String, font: TextStyle) -> El {
    const BLEED: f32 = 4.0;
    let name = text(title.clone()).font(font).color(p.text);
    let session = id.to_owned();
    row()
        .key(key.to_owned())
        .role("button")
        .label(title)
        .min_w(0.0)
        .shrink()
        .h(font.line_px())
        .gap(2.0)
        .center_y()
        .px(BLEED)
        .mx(-BLEED)
        .radius(4.0)
        .focus_inset(p)
        .hover_group()
        .hover(p.hover)
        .press(p.pressed)
        .hover_motion(Anim::INSTANT, Anim::INSTANT)
        .on_press_tx(move |s: &mut App, tx: &mut Tx| super::open_session(s, &server, &session, tx))
        .child(name.truncate().dir(Direction::Auto))
        .child(icon(&ic::OUTLINE_ARROW_UP_RIGHT, 14.0).color(p.icon_muted).group_color(p.icon))
}

/// A 28 pt ghost icon button, the height of the buttons beside it, its glyph `ink` under the
/// pointer.
pub(super) fn icon_button(p: &Pal, key: &str, label: &str, glyph: &'static rune::Icon, ink: rune::Color) -> El {
    row()
        .key(key.to_owned())
        .role("button")
        .label(label.to_owned())
        .help(label.to_owned())
        .size(28.0, 28.0)
        .center()
        .radius(6.0)
        .focus_default(p)
        .hover_group()
        .hover(p.hover)
        .press(p.pressed)
        .hover_motion(Anim::INSTANT, Anim::INSTANT)
        .child(icon(glyph, 16.0).color(p.icon_muted).group_color(ink))
}

/// The card a prompt, code or a run's output sits in: the text whole and selectable, folded past
/// [`fold::OVER`] lines, with its Copy button in the corner. `slot` names the card among its
/// automation's; `id` is that automation's key.
///
/// Official's box scrolls inside a 224 pt cap (`view.tsx:251`, `max-h-56 overflow-auto`), a
/// second scroller inside the inspector's own and no way to copy. This folds as the timeline
/// folds a long prompt, and gives the box official's `layer-01` with a hairline, which alone is
/// lost on a light base.
pub(super) fn body_card(s: &App, p: &Pal, id: &str, slot: &'static str, body: &str, code: bool) -> El {
    // The card's own insets around the button; the text adds its own to stand 14 pt in, its first
    // line centred on the button.
    const INSET: f32 = 8.0;
    const TEXT_X: f32 = 6.0;
    const TEXT_Y: f32 = 2.0;
    let style = if code { CODE } else { BODY };
    let clip_key = format!("{slot}-text");
    let width = rune::memo::layout_with(&clip_key, |r| r[2]).unwrap_or(480.0);
    let long = fold::lines(body, style, width) > fold::OVER;
    let fold_key = fold_key(id, slot);
    let open = s.kernel.slots.get(&AUTOMATIONS).folds.get(&fold_key) == Fold::Open;

    let words = text(body.to_owned())
        .font(style)
        .color(p.text)
        .w_full()
        .wrap_anywhere()
        .selectable(true)
        .dir(if code { Direction::Ltr } else { Direction::Auto });
    let mut clip = col().key(clip_key).w_full().child(words);
    if long && !open {
        clip = clip.max_h(fold::SHOW as f32 * style.line_px()).clip_fade(0.0, 0.0, 0.0, fold::FADE);
    }
    let mut main = col().grow(1.0).shrink().min_w(0.0).pl(TEXT_X).pt(TEXT_Y).child(clip);
    // The toggle's own height stands in for the text's inset under it.
    main = if long { main.child(fold_toggle(p, fold_key, open)) } else { main.pb(TEXT_Y) };
    row()
        .key(slot)
        .w_full()
        .items_start()
        .gap(INSET)
        .padding(INSET)
        .radius(8.0)
        .fill(p.layer1)
        .border(1.0, p.border_muted)
        .child(main)
        .child(copy_button(s, p, &format!("automation-copy:{id}:{slot}"), body))
}

/// The key of automation `id`'s card `slot` among the folds.
pub(super) fn fold_key(id: &str, slot: &str) -> String {
    format!("{id}\0{slot}")
}

/// Read more / Show less under a folded body, as the timeline's (`session_view::fold`).
fn fold_toggle(p: &Pal, fold_key: String, open: bool) -> El {
    let label = if open { t!("ui.message.showLess") } else { t!("ui.message.readMore") };
    row()
        .key("fold-toggle")
        .role("button")
        .label(label)
        .expanded(open)
        .self_start()
        .h(24.0)
        .mt(4.0)
        .gap(2.0)
        .center_y()
        .radius(4.0)
        .focus_default(p)
        .hover_group_focus()
        .hover_motion(Anim::INSTANT, Anim::INSTANT)
        .instant_changes()
        .on_press_tx(move |s: &mut App, tx: &mut Tx| {
            tx.instant();
            s.kernel.slots.get_mut(&AUTOMATIONS).folds.step(&fold_key, fold::Event::Toggle);
        })
        .child(text(label).font(BODY_MED).color(p.muted).group_color(p.text))
        .child(icon(&ic::CHEVRON_DOWN, 16.0).color(p.icon_muted).rotate(if open { std::f32::consts::PI } else { 0.0 }).rotate_motion(Anim::INSTANT))
}

/// The 24 pt ghost button that copies `body`, its glyph a tick for a moment after.
fn copy_button(s: &App, p: &Pal, copy_key: &str, body: &str) -> El {
    let done = s.copied(copy_key);
    let label = if done { t!("ui.message.copied") } else { t!("ui.message.copy") };
    let (copy_key, body) = (copy_key.to_owned(), body.to_owned());
    let glyph = crate::copied::glyph(s, &copy_key, 14.0, p.icon_muted);
    row()
        .key("copy")
        .role("button")
        .label(label)
        .help(label)
        .help_above()
        .help_open(done)
        .size(24.0, 24.0)
        .center()
        .radius(6.0)
        .focus_default(p)
        .hover(p.hover)
        .press(p.pressed)
        .hover_motion(Anim::INSTANT, Anim::INSTANT)
        .on_press_tx(move |s: &mut App, tx: &mut Tx| {
            tx.copy(body.clone());
            s.mark_copied(copy_key.clone(), tx);
        })
        .child(glyph)
}
