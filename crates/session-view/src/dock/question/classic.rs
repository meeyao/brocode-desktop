//! Desktop's default question dock, Classic (`session-question-dock-classic.tsx`,
//! `question-prompt.css`): a raised body with the question and its answers as bordered cards,
//! each with its mark and its number key, over a footer tray with Dismiss, Back and Next or
//! Submit. The header's chevron folds the dock down to its question.
//!
//! Keys: 1–9 pick, ↑/↓ (←/→, Home/End) move between answers, Space picks the one moved to, the
//! submit key (↵, or ⌘↵ / Ctrl+↵ by the setting) picks it and goes on or submits, the other Enter
//! only picks, ⌘↵ (Ctrl+↵) always goes on, ⌘[ (Alt+← off a Mac) goes back, Esc dismisses. In "Type
//! your own answer", keys type, the submit key goes on, the other Enter starts a new line, ↑ at
//! its start goes back to the answers and Esc leaves the field.

use std::f32::consts::PI;

use rune::{Anim, Color, Curve, Easing, El, Key, KeyEvent, Shadow, Spring, TextEdit, TextStyle, Tx, col, el, icon, row, spacer, text};
use rune_theme_opencode::icons as ic;

use super::*;
use crate::dock::{BUTTON, Variant, button, face_transition};
use crate::focus::FocusExt;
use crate::palette::{Elevate as _, Pal};
use crate::style::readable;

/// `question-header-title`: 13 px on the compact line height.
const HEAD: TextStyle = TextStyle::new(13.0, 440.0).lh(16.0);
pub(super) const QUESTION: TextStyle = TextStyle::new(13.0, 530.0).lh(16.0);
const HINT: TextStyle = TextStyle::new(13.0, 440.0).lh(20.0);
pub(super) const LABEL: TextStyle = TextStyle::new(13.0, 440.0).lh(20.0).tracking(-0.04);
/// `option-description`: compact leading (OpenCode ee5dfa9d41, `question-prompt.css`).
pub(super) const DESC: TextStyle = TextStyle::new(12.0, 440.0).lh(16.0).tracking(0.05);
/// `question-custom-input`: 13 px on an 18 px line (`question-prompt.css:397-420`).
pub(super) const INPUT: TextStyle = TextStyle::new(13.0, 440.0).lh(18.0);
/// A number key's cap (`keybind-v2` at the shortcut's 10 px, regular).
const KEY: TextStyle = TextStyle::new(10.0, 400.0).lh(11.0).tracking(0.05).features("\"tnum\", \"lnum\"");
fn custom_label() -> &'static str {
    opencode_i18n::t!("ui.question.custom.answerPlaceholder")
}
/// The footer tray's height: 16 pt over and 8 pt under the 32 pt buttons, inside its borders.
const TRAY_H: f32 = 57.0;
/// How far the tray tucks up under the body (`margin-block-start: -9px`).
const TUCK: f32 = 9.0;
/// The gap the dock keeps from the top of the pane (`measure`'s `gap`), and its floor.
pub(super) const PANE_GAP: f32 = 8.0;
pub(super) const MIN_H: f32 = 240.0;
/// The fold's spring (`useSpring`, 0.3 s without bounce).
const FOLD: Spring = Spring::new(0.3, 0.0);
const MAC: bool = cfg!(target_os = "macos");

pub fn face<S: DockHost>(s: &S, p: &Pal) -> Option<Vec<El>> {
    let f = asking(s)?;
    let (tab, focus) = place(s, &f);
    let field = &f.fields[tab];
    let multi = f.multi(tab);
    let k = f.key(tab);
    let editing = editing(s, &f, tab);
    let list = rows(field);
    let picks = s.dock().picks.get(&k).cloned().unwrap_or_default();
    let total = f.fields.len();
    let d = s.dock();
    let minimized = d.minimized;
    let ring = d.keyboard && !editing;
    let fade = || face_transition(s.knobs());
    let max = s.dock_pane_h().map(|h| (h - PANE_GAP).max(MIN_H));
    let live = !super::busy(s, &f);

    let title = if total == 1 { opencode_i18n::t!("session.question.title").to_owned() } else { opencode_i18n::t!("session.question.progress", current = tab + 1, total = total) };
    let top = row()
        .key("q-top")
        .w_full()
        .min_h(28.0)
        .gap(12.0)
        .center_y()
        .child(text(title).key("q-title").font(HEAD).color(p.muted).wrap_anywhere().shrink())
        .child(spacer())
        .child(row().key("q-header-actions").gap(8.0).center_y().children((total > 1).then(|| progress(s, p, &f, tab, live))).child(chevron::<S>(p, minimized, live)));
    let text_of = if field.description.is_empty() { &field.title } else { &field.description };
    let question = text(text_of.clone()).key("q-text").font(QUESTION).color(p.text).wrap_anywhere();
    let question = readable(p, if minimized { question.max_lines(3) } else { question });
    let hint = if multi { opencode_i18n::t!("ui.question.classic.multiHint") } else { opencode_i18n::t!("ui.question.singleHint") };
    // No bar, as Desktop hides it (`question-prompt.css:77-85`).
    let text_and_hint = col().w_full().gap(8.0).child(question).children((!minimized).then(|| text(hint).key("q-hint").font(HINT).color(p.faint).wrap()));
    let details = crate::scroller::scroller(format!("q-details-{tab}")).hidden(col().w_full().shrink().min_h(0.0), text_and_hint);
    let details = match max {
        Some(m) => details.max_h(((m - 120.0) * 0.45).max(48.0)),
        None => details,
    };
    let header = col().key("q-header").w_full().gap(4.0).child(top).child(details);

    // Folded, the answers close to nothing but keep their place, and the body's gap above them.
    let folded = minimized.then(|| el().key("q-folded").w_full().h(0.0));
    let options = (!minimized).then(|| {
        // Answer cards are focusable `on_press` elements, so under `web_focus` Rune activates
        // their `on_press` on unmodified Enter/Space before the window's `on_key` sees it.
        // Handling option keys on the group lets Enter submit (or select per setting) first;
        // Rune skips `El::on_key` while the typed-answer text field `q-input` has focus.
        let mut answers = col()
            .key(format!("q-options-{tab}"))
            .w_full()
            .gap(6.0)
            .role(if multi { "group" } else { "radiogroup" })
            .on_key(|s: &mut S, ev, tx| super::key(s, ev, tx));
        for (i, (value, label, desc)) in list.iter().enumerate() {
            let picked = picks.contains(value);
            let card = card(p, picked, ring && focus == i, d.keyboard)
                .key(format!("q-opt-{i}"))
                .role(if multi { "checkbox" } else { "radio" })
                .label(label.clone())
                .checked(picked)
                // The card draws its own keyboard ring (`ring`), which follows Rune's focus.
                .no_focus_ring()
                .items_start()
                .padding(10.0)
                .child(mark(p, multi, picked, d.keyboard))
                .child(
                    col()
                        .key("main")
                        .grow(1.0)
                        .shrink()
                        .gap(4.0)
                        .child(text(label.clone()).key("label").dir(rune::Direction::Auto).font(LABEL).color(p.text).wrap_anywhere())
                        .children((!desc.is_empty()).then(|| text(desc.clone()).key("description").dir(rune::Direction::Auto).font(DESC).color(p.muted).wrap_anywhere())),
                )
                .children((i < 9).then(|| shortcut(p, i + 1, picked, d.keyboard)));
            let card = if live {
                card.on_press_tx(move |s: &mut S, tx: &mut Tx| {
                    let Some(f) = asking(s) else { return };
                    let (tab, _) = place(s, &f);
                    select(s, &f, tab, i);
                    let d = dock(s, &f);
                    (d.focus, d.keyboard) = (i, tx.focus_visible());
                })
            } else {
                card.disabled(true).focusable(false)
            };
            answers = answers.child(card);
        }
        if has_custom(field) {
            answers = answers.child(custom(s, p, &f, tab, multi, editing, ring && focus >= list.len()));
        }
        // No bar, as Desktop hides it (`question-prompt.css:186-194`).
        // Room for the picked card's shadow, which the scroll box would clip (`margin: -4px; padding: 4px`).
        crate::scroller::scroller(format!("q-scroll-{tab}")).hidden(col().w_full().shrink().min_h(0.0).mx(-4.0).my(-4.0).transition(fade()), answers.padding(4.0))
    });

    let body = col()
        .key("question")
        .w_full()
        .pt(8.0)
        .px(12.0)
        .pb(12.0)
        .gap(16.0)
        .radius(10.0)
        .fill(p.bg_base)
        .transition(fade())
        .child(header)
        .children(options)
        .children(folded);
    let body = body.elevate(p.raised);
    let body = match max {
        Some(m) => body.max_h((m - (TRAY_H - TUCK)).max(0.0)),
        None => body,
    };

    let last = f.last(tab);
    // Both labels keep their room so the button doesn't change width between Next and Submit.
    let advance_w = [opencode_i18n::t!("ui.common.next"), opencode_i18n::t!("ui.common.submit")].map(|l| rune::text::measure(l, &[], BUTTON, None).0).into_iter().fold(0.0, f32::max) + 30.0;
    let submit = submit_hint(s.dock_question_submit());
    let back = if MAC { "⌘[".to_owned() } else { format!("{}+←", opencode_i18n::t!("common.key.alt")) };
    let footer = row()
        .key("question-footer")
        .w_full()
        .mt(-TUCK)
        .pt(16.0)
        .pb(8.0)
        .px(8.0)
        // The tray (`question-footer`): `layer-01` with `border: 0.5px`, which Chromium draws on
        // whole device pixels and lays out as such (`border_layout`: 1 pt each side at 1x, 0.667
        // at 1.5x). It sits under the body's bottom edge (`paint_order`), so the body's shadow
        // falls on it.
        .corners([0.0, 0.0, 10.0, 10.0])
        .fill(p.layer1)
        .border_sides([0.5, 0.5, 0.5, 0.5], p.border)
        .border_layout()
        .paint_order(-1)
        .center_y()
        .transition(fade())
        .child({
            let b = button(p, "q-dismiss", opencode_i18n::t!("ui.common.dismiss"), Variant::GhostMuted, None, live).help(format!("{} Esc", opencode_i18n::t!("ui.common.dismiss"))).help_above();
            if live { b.on_press_tx(|s: &mut S, tx: &mut Tx| if let Some(f) = asking(s) { dismiss(s, &f, tx) }) } else { b }
        })
        .child(spacer())
        .child(
            row()
                .key("q-actions")
                .gap(8.0)
                .children((tab > 0).then(|| {
                    let b = button(p, "q-back", opencode_i18n::t!("ui.common.back"), Variant::Neutral, None, live).help(format!("{} {back}", opencode_i18n::t!("ui.common.back"))).help_above();
                    if live {
                        b.on_press_tx(|s: &mut S, tx: &mut Tx| {
                            let Some(f) = asking(s) else { return };
                            let (tab, _) = place(s, &f);
                            jump(s, &f, tab.saturating_sub(1), tx);
                        })
                    } else {
                        b
                    }
                }))
                .child({
                    let label = if last { opencode_i18n::t!("ui.common.submit") } else { opencode_i18n::t!("ui.common.next") };
                    let b = button(p, "q-next", label, if last { Variant::Contrast } else { Variant::Neutral }, None, live)
                        .w(advance_w)
                        .justify_center()
                        .help(format!("{label} {submit}"))
                        .help_above();
                    if live { b.on_press_tx(|s: &mut S, tx: &mut Tx| if let Some(f) = asking(s) { next(s, &f, tx) }) } else { b }
                }),
        );
    Some(vec![body, footer])
}

/// How the cards' feedback eases (`question-prompt.css`: `--question-feedback-duration: 120ms`,
/// `ease`), and at once while keys drive the dock (`[data-keyboard="true"]` sets it to 0s).
pub(super) fn feedback(keyboard: bool) -> Anim {
    if keyboard { Anim::INSTANT } else { Anim::new(Curve::tween(0.12, Easing::EASE)) }
}

/// An answer's card (`question-option`): `bg-base` under a hairline; picked, a raised card on a
/// `layer-02` wash; under the pointer, `layer-01` (anomalyco/opencode#66169 swapped the two,
/// `question-prompt.css:2`, `:488`); moved to by key, the focus ring. The
/// hover's `background-color` and the wash's opacity (its `::before`) both ease on `feedback`;
/// the hairline goes at once (`box-shadow` isn't transitioned). The hover is the pointer's own
/// track, so moving over the answers commits nothing.
fn card(p: &Pal, picked: bool, ring: bool, keyboard: bool) -> El {
    let ease = feedback(keyboard);
    let wash = p.layer2;
    // Its hairline, elevation and ring change at once (`Spring::NONE`): only the hover and the wash ease.
    let c = row().w_full().gap(8.0).radius(6.0).fill(p.bg_base).motion(Spring::NONE).hover_motion(ease, ease);
    let c = if picked {
        // `--v2-elevation-raised`; CSS paints its first shadow on top, so the last goes first.
        let [far, near, edge_ring, edge] = p.raised;
        c.box_shadow(edge).box_shadow(edge_ring).box_shadow(near).box_shadow(far)
    } else {
        c.hover(p.layer1).box_shadow(Shadow { spread: 0.5, color: p.border, inset: true, ..Shadow::default() })
    };
    // Under a contrast theme every card has a solid edge, so the ring is 3 wide to stand clear of it.
    let c = if ring { c.box_shadow(Shadow { spread: if p.forced { 3.0 } else { crate::focus::WIDTH }, color: p.q.focus, inset: true, ..Shadow::default() }) } else { c };
    c.child(el().key("wash").absolute(0.0, 0.0).w_full().h_full().radius(6.0).fill(wash).opacity(if picked { 1.0 } else { 0.0 }).opacity_motion(ease))
}

/// The header's progress: a bar per question, accent once answered, muted on the current one.
pub(super) fn progress<S: DockHost>(s: &S, p: &Pal, f: &Form, tab: usize, live: bool) -> El {
    row().key("q-progress").gap(2.0).center_y().children((0..f.fields.len()).map(|i| {
        let color = if i == tab { p.icon_muted } else if answered(s, f, i) { p.bg_accent } else { p.q.layer4 };
        let seg = el()
            .key(format!("q-seg-{i}"))
            .role("button")
            .label(opencode_i18n::t!("ui.tool.questions.numbered", number = i + 1))
            .size(20.0, 20.0)
            .radius(4.0)
            .focus_inset(p)
            .center()
            .child(el().key("bar").size(14.0, 2.0).fill(color));
        if live {
            seg.hover(p.hover).on_press_tx(move |s: &mut S, tx: &mut Tx| {
                if let Some(f) = asking(s) {
                    jump(s, &f, i, tx)
                }
            })
        } else {
            seg.disabled(true).focusable(false).hit().cursor(rune::Cursor::NotAllowed)
        }
    }))
}

/// The header's chevron (`icon-button-v2`, ghost, large): folds the dock to its question and back,
/// turning over as it does.
fn chevron<S: DockHost>(p: &Pal, minimized: bool, live: bool) -> El {
    let label = if minimized { opencode_i18n::t!("session.question.restore") } else { opencode_i18n::t!("session.question.minimize") };
    let b = el()
        .key("q-minimize")
        .role("button")
        .label(label)
        .expanded(!minimized)
        .size(28.0, 28.0)
        .radius(6.0)
        .center()
        .child(icon(&ic::CHEVRON_DOWN, 16.0).color(p.icon_muted).rotate(if minimized { PI } else { 0.0 }).rotate_motion(FOLD));
    if live {
        b.focus_default(p).help(label).help_above().hover(p.hover).press(p.pressed).on_press_tx(|s: &mut S, tx: &mut Tx| {
            let Some(f) = asking(s) else { return };
            tx.motion_curve(FOLD);
            let (tab, _) = place(s, &f);
            stop_typing(s, &f, tab);
            let focus = pick_focus(s, &f, tab);
            let d = dock(s, &f);
            d.minimized = !d.minimized;
            d.focus = focus;
            d.keyboard = false;
        })
    } else {
        b.opacity(0.5).disabled(true).focusable(false)
    }
}

/// An answer's mark (`question-option-check`): a circle for one answer, a square for many,
/// accent with a dot or a tick once picked.
pub(super) fn mark(p: &Pal, multi: bool, picked: bool, keyboard: bool) -> El {
    // The box's fill isn't transitioned: it turns accent at once (`Spring::NONE`).
    let b = el().key("box").size(16.0, 16.0).center().radius(if multi { 4.0 } else { 8.0 }).motion(Spring::NONE);
    let b = if picked {
        b.fill(p.bg_accent).gradient(Color::WHITE.alpha(0.2), Color::WHITE.alpha(0.0))
    } else {
        b.fill(p.bg_base).gradient(Color::WHITE.alpha(0.06), Color::WHITE.alpha(0.0))
    };
    // A picked circle drops its hairline; a picked square keeps it.
    let b = if picked && !multi { b } else { b.box_shadow(Shadow { spread: 0.5, color: p.q.border_strong, inset: true, ..Shadow::default() }) };
    // The dot and the tick are always there, shown once picked, as Desktop fades them (`opacity`
    // on `feedback`); the box's own fill changes at once.
    let inner = if multi {
        icon(&ic::CHECK, 16.0).key("check").color(p.icon_contrast)
    } else {
        el().key("dot").size(6.0, 6.0).radius(3.0).fill(p.q.dot).box_shadow(Shadow { y: 0.5, blur: 0.5, color: Color::BLACK.alpha(0.4), ..Shadow::default() })
    };
    el().key("mark").size(16.0, 20.0).center().child(b.child(inner.opacity(if picked { 1.0 } else { 0.0 }).opacity_motion(feedback(keyboard))))
}

/// An answer's number key (`keybind-v2`): filled, or outlined once picked. Its `background-color`
/// and `box-shadow` ease on `feedback` (`--keybind-transition-duration`, `keybind.css`): the fill
/// and the outline are two layers that cross-fade.
pub(super) fn shortcut(p: &Pal, n: usize, picked: bool, keyboard: bool) -> El {
    let ease = feedback(keyboard);
    let layer = |key: &'static str, on: bool| el().key(key).absolute(0.0, 0.0).w_full().h_full().radius(2.0).opacity(if on { 1.0 } else { 0.0 }).opacity_motion(ease);
    let fill = layer("fill", !picked).fill(p.layer3);
    let ring = layer("ring", picked).box_shadow(Shadow { spread: 1.0, color: p.q.border_strong, inset: true, ..Shadow::default() });
    let cap = row().key("cap").h(14.0).min_w(14.0).px(4.0).center().radius(2.0).child(fill).child(ring);
    row().key("shortcut").h(20.0).self_center().center_y().child(cap.child(text(n.to_string()).font(KEY).color(p.muted)))
}

/// "Type your own answer" (`question-option[data-custom]`): its mark, the field in an answer's
/// place, and its number key. The field is of several lines and grows with its text
/// (anomalyco/opencode#66169: a `textarea`, `question-prompt.css:361-420`).
fn custom<S: DockHost>(s: &S, p: &Pal, f: &Form, tab: usize, multi: bool, editing: bool, ring: bool) -> El {
    let live = !super::busy(s, f);
    let field = &f.fields[tab];
    let k = f.key(tab);
    let i = rows(field).len();
    let on = s.dock().custom_on(&k);
    let edit = s.dock().typed.get(&k).cloned().unwrap_or_else(|| TextEdit::new(""));
    let (id, field_key) = (f.id.clone(), field.key.clone());
    let input = crate::style::field(&edit, INPUT)
        .key("q-input")
        .spellcheck(s.composer_spellcheck())
        .label(custom_label())
        // `dir="auto"` (`session-question-dock-classic.tsx:725`).
        .dir(rune::Direction::Auto)
        .w_full()
        .color(p.text)
        .placeholder(custom_label(), if editing { p.faint } else { p.text })
        .caret(p.text, p.selection).selection_style(p.field_selection)
        .on_edit(move |s: &mut S| s.dock_mut().typed_mut(&id, &field_key))
        .on_input_change(|s: &mut S, _| {
            let Some(f) = asking(s) else { return };
            let (tab, _) = place(s, &f);
            let k = f.key(tab);
            let d = s.dock_mut();
            d.custom.insert(k.clone());
            if !f.multi(tab) {
                d.picks.remove(&k);
            }
        })
        .on_input_key(|s: &mut S, ev, tx| typed_key(s, ev, tx));
    // Six lines at most, then it scrolls inside itself, with a plain `overflow-y: auto` box's bar.
    let input = crate::scroller::scroller(format!("q-input-scroll-{tab}")).build::<S>(p, col().grow(1.0).min_w(0.0).max_h(FIELD_MAX_H), input);
    // Open, the field gets a box of its own with the focus ring. 5 over and under its 18 pt
    // lines: 28 tall on one line, taller as it wraps or takes a new line.
    let input = if editing {
        let lit = Color::WHITE.alpha(0.02);
        row()
            .key("q-field")
            .grow(1.0)
            .min_w(0.0)
            .min_h(28.0)
            .px(8.0)
            .pt(5.0)
            .pb(5.0)
            .radius(6.0)
            .fill(p.bg_base)
            .gradient(lit, Color::WHITE.alpha(0.0))
            .box_shadow(Shadow { spread: 1.0, color: p.q.focus, inset: true, ..Shadow::default() })
            .child(input)
    } else {
        row().key("q-field").grow(1.0).min_w(0.0).min_h(28.0).pt(5.0).pb(5.0).child(input)
    };
    let keyboard = s.dock().keyboard;
    // `question-custom-toggle`: 4 under the row's top, level with the field's first line.
    let mark_el = mark(p, multi, on, keyboard).key("q-custom-mark").mt(4.0).role(if multi { "checkbox" } else { "radio" }).label(custom_label());
    let mark_el = if live {
        mark_el.cursor(rune::Cursor::Pointer).on_press_tx(|s: &mut S, _: &mut Tx| {
            let Some(f) = asking(s) else { return };
            let (tab, _) = place(s, &f);
            toggle_custom(s, &f, tab);
            dock(s, &f).keyboard = false;
        })
    } else {
        mark_el.disabled(true).focusable(false)
    };
    let row_card = card(p, on, ring, keyboard)
        .key("q-custom")
        .role(if multi { "checkbox" } else { "radio" })
        .label(custom_label())
        .checked(on)
        .no_focus_ring()
        // `align-items: flex-start; min-block-size: 40px; padding-block: 6px`.
        .min_h(40.0)
        .px(10.0)
        .pt(6.0)
        .pb(6.0)
        .items_start()
        .child(mark_el)
        .child(input)
        .children((i < 9).then(|| shortcut(p, i + 1, on, keyboard)));
    if live {
        row_card.on_press_tx(|s: &mut S, _: &mut Tx| {
            let Some(f) = asking(s) else { return };
            let (tab, _) = place(s, &f);
            start_typing(s, &f, tab);
            dock(s, &f).keyboard = false;
        })
    } else {
        row_card.disabled(true).focusable(false)
    }
}

/// Back: ⌘[ on a Mac, Alt+← elsewhere.
pub(super) fn back(ev: &KeyEvent) -> bool {
    let m = ev.mods;
    if MAC {
        return matches!(ev.key, Key::Command('[')) && !m.ctrl && !m.alt;
    }
    ev.key == Key::Left && m.alt && !m.ctrl && !m.cmd
}

pub(super) fn advance(ev: &KeyEvent) -> bool {
    ev.key == Key::Enter && (ev.mods.cmd || ev.mods.ctrl) && !ev.mods.alt
}

/// A key while the Classic dock shows (its `nav`); false when it isn't the dock's.
pub fn key<S: DockHost>(s: &mut S, ev: &KeyEvent, tx: &mut Tx) -> bool {
    let Some(f) = asking(s) else { return false };
    if !super::focus_in_dock(tx.focused()) {
        return false;
    }
    let (tab, focus) = place(s, &f);
    let field = &f.fields[tab];
    let n = rows(field).len();
    let focus = focused_option(tx.focused(), n, has_custom(field)).unwrap_or(focus);
    let count = count(field);
    let minimized = s.dock().minimized;
    // A reply on its way: Esc, Back and the advance key do nothing, and the rest aren't the dock's.
    if super::busy(s, &f) {
        return ev.key == Key::Escape || back(ev) || advance(ev);
    }
    if ev.key == Key::Escape {
        dock(s, &f).keyboard = true;
        dismiss(s, &f, tx);
        return true;
    }
    if back(ev) {
        dock(s, &f).keyboard = true;
        if tab > 0 {
            jump(s, &f, tab - 1, tx);
        }
        return true;
    }
    if advance(ev) {
        dock(s, &f).keyboard = true;
        next(s, &f, tx);
        return true;
    }
    let m = ev.mods;
    if editing(s, &f, tab) || m.alt || m.ctrl || m.cmd || matches!(ev.key, Key::Command(_)) {
        return false;
    }
    let digit = match &ev.key {
        Key::Text(t) if t.chars().count() == 1 => t.chars().next().and_then(|c| c.to_digit(10)).filter(|d| *d >= 1).map(|d| d as usize - 1),
        _ => None,
    };
    if let Some(i) = digit {
        if minimized || i >= count {
            return false;
        }
        dock(s, &f).keyboard = true;
        // A pick's mark fades in, and opening the typed answer's field moves its words in by the
        // field's padding, on the springs rather than at once.
        if i == n {
            toggle_custom(s, &f, tab);
        } else {
            select(s, &f, tab, i);
            dock(s, &f).focus = i;
        }
        reveal(s, tx);
        return true;
    }
    // The rest move and pick among the answers, which a folded dock hides; header and footer
    // buttons keep their own Space/Enter/arrows (`session-question-dock-classic.tsx:354-396`).
    if minimized || !focus_in_options(tx.focused()) {
        return false;
    }
    let rtl = opencode_i18n::direction() == opencode_i18n::Direction::Rtl;
    let to = match &ev.key {
        Key::Left if rtl => Some((focus + 1).min(count - 1)),
        Key::Right if rtl => Some(focus.saturating_sub(1)),
        Key::Down | Key::Right => Some((focus + 1).min(count - 1)),
        Key::Up | Key::Left => Some(focus.saturating_sub(1)),
        Key::Home => Some(0),
        Key::End => Some(count - 1),
        _ => None,
    };
    if let Some(to) = to {
        tx.instant();
        let d = dock(s, &f);
        (d.focus, d.keyboard) = (to, true);
        // The arrows and End land in the typed answer's field rather than on its mark, so the
        // answer is typed at once (`session-question-dock-classic.tsx:235-247`, `:335`, `:421`).
        if to >= n && ev.key != Key::Home {
            enter_field(s, &f, tab);
        }
        reveal(s, tx);
        return true;
    }
    let space = matches!(&ev.key, Key::Text(t) if t == " ");
    let enter = enter(ev, s.dock_question_submit());
    if !space && enter.is_none() {
        return false;
    }
    if tx.key_repeat() {
        return true;
    }
    dock(s, &f).keyboard = true;
    match enter {
        // The submit key on an answer picks it first on a single choice; on many, it moves on
        // with the answers already marked (`session-question-dock-classic.tsx:440-445`).
        Some(Enter::Submit) if focus < n => {
            if !f.multi(tab) {
                select(s, &f, tab, focus);
            }
            next(s, &f, tx);
        }
        // On the typed answer's mark it opens the field while that is still empty on a single
        // choice, since there is nothing to pick yet (`:449-457`).
        Some(Enter::Submit) if f.multi(tab) => next(s, &f, tx),
        Some(Enter::Submit) if s.dock().typed.get(&f.key(tab)).is_none_or(|t| t.text.trim().is_empty()) => toggle_custom(s, &f, tab),
        Some(Enter::Submit) => {
            let k = f.key(tab);
            let d = s.dock_mut();
            d.custom.insert(k.clone());
            d.picks.remove(&k);
            next(s, &f, tx);
        }
        // Space and the other Enter only pick.
        _ if focus >= n => toggle_custom(s, &f, tab),
        _ => select(s, &f, tab, focus),
    }
    true
}

/// Puts the caret in the typed answer's field without picking that answer: typing in it does
/// (`session-question-dock-classic.tsx:730-735`).
pub(super) fn enter_field<S: DockHost>(s: &mut S, f: &Form, tab: usize) {
    s.composer_edit_mut().focused = false;
    s.dock_mut().typed_mut(&f.id, &f.fields[tab].key).focused = true;
}

/// Keys in the open "Type your own answer" (`session-question-dock-classic.tsx:738-771`): Esc
/// leaves the field for its card, ⌘↵ and Back as anywhere, ↑ at the field's start goes back to
/// the answers above it, the submit key moves on; everything else types, the other Enter a new
/// line.
fn typed_key<S: DockHost>(s: &mut S, ev: &KeyEvent, tx: &mut Tx) -> bool {
    let Some(f) = asking(s) else { return false };
    let (tab, _) = place(s, &f);
    let n = rows(&f.fields[tab]).len();
    let m = ev.mods;
    if ev.key == Key::Escape {
        stop_typing(s, &f, tab);
        let d = dock(s, &f);
        (d.focus, d.keyboard) = (n, true);
        reveal(s, tx);
        return true;
    }
    if advance(ev) || back(ev) {
        stop_typing(s, &f, tab);
        return key(s, ev, tx);
    }
    let at_start = s.dock().typed.get(&f.key(tab)).is_none_or(|t| t.focus == 0 && t.anchor == 0);
    if ev.key == Key::Up && n > 0 && at_start && !m.shift && !m.alt && !m.cmd && !m.ctrl {
        tx.instant();
        stop_typing(s, &f, tab);
        let d = dock(s, &f);
        (d.focus, d.keyboard) = (n - 1, true);
        reveal(s, tx);
        return true;
    }
    if !m.cmd && !m.ctrl {
        match enter(ev, s.dock_question_submit()) {
            Some(Enter::Submit) => {
                if !tx.key_repeat() {
                    dock(s, &f).keyboard = true;
                    next(s, &f, tx);
                }
                return true;
            }
            Some(Enter::Select) if !m.shift => {
                insert_typed_newline(s, &f, tab);
                return true;
            }
            _ => {}
        }
    }
    s.dock_shortcut(ev, tx)
}

/// Brings the highlighted answer into sight in the answers' scroller.
fn reveal<S: DockHost>(s: &S, tx: &mut Tx) {
    let Some(f) = asking(s) else { return };
    let (tab, focus) = place(s, &f);
    let field = &f.fields[tab];
    let key = if focus >= rows(field).len() && has_custom(field) { "q-custom".to_owned() } else { format!("q-opt-{focus}") };
    // Keyboard focus goes with the dock's own, so a reader says the answer moved to.
    tx.focus(key.clone(), true);
    tx.scroll_into_view_in(format!("q-scroll-{tab}"), key);
}
