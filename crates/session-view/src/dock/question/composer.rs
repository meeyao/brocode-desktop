//! Composer-integrated question dock (`session-question-dock-classic.tsx` with `composer=true`,
//! `question-answer-composer.css`, `opencode#66825`): preset answers in a raised card above,
//! and a composer-styled custom answer field with the single Next/Send icon button 8 pt below.

use rune::{Anim, Color, El, Key, KeyEvent, Shadow, Spring, TextEdit, TextStyle, Tx, col, el, icon, row, text};
use rune_theme_opencode::icons as ic;

use super::classic::{self, DESC, LABEL, MIN_H, PANE_GAP, QUESTION, advance, back, enter_field, feedback, progress, shortcut};
use super::*;
use crate::dock::{face_transition, flat};
use crate::focus::FocusExt;
use crate::palette::{Elevate as _, Pal};
use crate::style::readable;

/// Composer custom-answer input: 13 px on `--line-height-base` (20 px, `question-answer-composer.css:164`).
const COMPOSER_INPUT: TextStyle = TextStyle::new(13.0, 440.0).lh(20.0);
/// Approximate composer footer card height (8 pt pad + 28 pt min row + 8 pt pad + 8 pt gap).
const FOOTER_H: f32 = 52.0;

fn answer_label() -> &'static str {
    opencode_i18n::t!("ui.question.custom.placeholder")
}

/// The active option index in composer mode: explicit `active_option` when browsing the list,
/// else the resting option when focus is outside the composer field.
fn active_option<S: DockHost>(s: &S, f: &Form, tab: usize, n: usize, editing_here: bool) -> Option<usize> {
    if n == 0 || editing_here {
        return None;
    }
    let d = s.dock();
    if d.form == f.id {
        d.active_option.filter(|&i| i < n)
    } else {
        Some(resting_option(s, f, tab).min(n - 1))
    }
}

/// The answer the list highlights when it takes focus: the first picked one, or the first
/// (`session-question-dock-classic.tsx:287-292`).
fn resting_option<S: DockHost>(s: &S, f: &Form, tab: usize) -> usize {
    let k = f.key(tab);
    let picks = s.dock().picks.get(&k);
    rows(&f.fields[tab])
        .iter()
        .position(|(v, ..)| picks.is_some_and(|p| p.contains(v)))
        .unwrap_or(0)
}

pub fn face<S: DockHost>(s: &S, p: &Pal) -> Option<Vec<El>> {
    let f = asking(s)?;
    let (tab, _) = place(s, &f);
    let field = &f.fields[tab];
    let multi = f.multi(tab);
    let k = f.key(tab);
    let editing_here = editing(s, &f, tab);
    let list = rows(field);
    let n = list.len();
    let picks = s.dock().picks.get(&k).cloned().unwrap_or_default();
    let total = f.fields.len();
    let d = s.dock();
    let active = active_option(s, &f, tab, n, editing_here);
    let fade = || face_transition(s.knobs());
    let max = s.dock_pane_h().map(|h| (h - PANE_GAP).max(MIN_H));
    let live = !super::busy(s, &f);

    let text_of = if field.description.is_empty() { &field.title } else { &field.description };
    let question = readable(p, text(text_of.clone()).key("q-text").font(QUESTION).color(p.text).wrap_anywhere());
    // `question-details`: first 16 pt line centred on the 28 pt header controls (`padding-block-start: 6px`).
    let details = crate::scroller::scroller(format!("q-details-{tab}"))
        .hidden(col().w_full().shrink().min_h(0.0).pt(6.0), question);
    let details = match max {
        Some(m) => details.max_h(((m - 120.0) * 0.45).max(48.0)),
        None => details,
    };

    // `question-header-top`: shifted 3 pt toward end so `×` aligns over number badges (`question-answer-composer.css:22-27`).
    let dismiss_label = opencode_i18n::t!("ui.common.dismiss");
    let dismiss_btn = el()
        .key("q-dismiss")
        .role("button")
        .label(dismiss_label)
        .size(28.0, 28.0)
        .radius(6.0)
        .center()
        .child(icon(&ic::CLOSE, 16.0).color(p.icon_muted));
    let dismiss_btn = if live {
        dismiss_btn
            .focus_default(p)
            .help(format!("{dismiss_label} Esc"))
            .help_above()
            .hover(p.hover)
            .press(p.pressed)
            .on_press_tx(|s: &mut S, tx: &mut Tx| {
                if let Some(f) = asking(s) {
                    dismiss(s, &f, tx);
                }
            })
    } else {
        dismiss_btn.opacity(0.5).disabled(true).focusable(false)
    };

    let header = row()
        .key("q-header")
        .w_full()
        .gap(12.0)
        .items_start()
        .child(col().key("q-details-wrap").grow(1.0).shrink().min_w(0.0).child(details))
        .child(
            row()
                .key("q-header-actions")
                .mr(-3.0)
                .gap(8.0)
                .center_y()
                .children((total > 1).then(|| progress(s, p, &f, tab, live)))
                .child(dismiss_btn),
        );

    let first_has_desc = list.first().is_some_and(|(_, _, desc)| !desc.is_empty());
    let options = (n > 0).then(|| {
        let mut answers = col()
            .key(format!("q-options-{tab}"))
            .w_full()
            .gap(1.0)
            .role("listbox")
            .label(text_of.clone())
            .focusable(live)
            .no_focus_ring()
            .on_key(|s: &mut S, ev, tx| key(s, ev, tx));
        for (i, (value, label, desc)) in list.iter().enumerate() {
            let picked = picks.contains(value);
            let prev_picked = i > 0 && picks.contains(&list[i - 1].0);
            let next_picked = i + 1 < n && picks.contains(&list[i + 1].0);
            let is_active = active == Some(i);
            let row_el = option_row(p, picked, is_active, prev_picked, next_picked, d.keyboard, desc.is_empty())
                .key(format!("q-opt-{i}"))
                .role("option")
                .label(label.clone())
                .checked(picked)
                .no_focus_ring()
                .child(option_mark(p, multi, picked, d.keyboard))
                .child(
                    col()
                        .key("main")
                        .grow(1.0)
                        .shrink()
                        .gap(2.0)
                        .child(text(label.clone()).key("label").dir(rune::Direction::Auto).font(LABEL).color(p.text).wrap_anywhere())
                        .children((!desc.is_empty()).then(|| text(desc.clone()).key("description").dir(rune::Direction::Auto).font(DESC).color(p.muted).wrap_anywhere())),
                )
                .children((i < 9).then(|| shortcut(p, i + 1, picked, d.keyboard)));
            let row_el = if live {
                row_el
                    .on_hover_change(move |s: &mut S, inside: bool, _tx: &mut Tx| {
                        if inside && let Some(f) = asking(s) {
                            let d = dock(s, &f);
                            d.active_option = Some(i);
                            d.keyboard = false;
                        }
                    })
                    .on_press_tx(move |s: &mut S, tx: &mut Tx| {
                        let Some(f) = asking(s) else { return };
                        let (tab, _) = place(s, &f);
                        select(s, &f, tab, i);
                        let d = dock(s, &f);
                        d.focus = i;
                        d.active_option = Some(i);
                        d.keyboard = tx.focus_visible();
                    })
            } else {
                row_el.disabled(true).focusable(false)
            };
            answers = answers.child(row_el);
        }
        // Highlight reaches 6 pt from card edges while answers stay on the 16 pt line (`question-answer-composer.css:46`).
        crate::scroller::scroller(format!("q-scroll-{tab}"))
            .hidden(col().w_full().shrink().min_h(0.0).ml(-10.0).mr(-6.0).transition(fade()), answers)
    });

    let body = col()
        .key("question")
        .w_full()
        .pt(10.0)
        .pb(6.0)
        .pl(16.0)
        .pr(16.0)
        .gap(if first_has_desc { 4.0 } else { 2.0 })
        .radius(10.0)
        .fill(p.bg_base)
        .transition(fade())
        .child(header)
        .children(options)
        .elevate(p.raised);
    let body = match max {
        Some(m) => body.max_h((m - FOOTER_H).max(0.0)),
        None => body,
    };

    // Bottom composer card (`question-answer-composer.css:139-171`): raised shadow, no border,
    // 12 pt radius, `bg-layer-01` in dark / `bg-base` in light, 8 pt below the preset card.
    // Keep `TextEdit::focused` (and any active IME preedit) even while `!on`: Rune drops focus
    // on commit whenever a text input renders with `edit.focused == false` (`focus.rs:565`).
    let on = s.dock().custom_on(&k);
    let shown_edit = match s.dock().typed.get(&k) {
        Some(t) if on || t.text.is_empty() => t.clone(),
        Some(t) => {
            let mut blank = TextEdit::new("");
            blank.focused = t.focused;
            blank
        }
        None => TextEdit::new(""),
    };
    let (form_id, field_key) = (f.id.clone(), field.key.clone());
    let mut input = crate::style::field(&shown_edit, COMPOSER_INPUT)
        .key("q-input")
        .spellcheck(s.composer_spellcheck())
        .label(answer_label())
        .dir(rune::Direction::Auto)
        .w_full()
        .color(p.text)
        .placeholder(answer_label(), p.faint)
        .caret(p.text, p.selection)
        .selection_style(p.field_selection);
    if live {
        input = input
            .on_edit(move |s: &mut S| {
                let k = format!("{form_id}/{field_key}");
                let d = s.dock_mut();
                if !d.custom_on(&k) && d.typed.get(&k).is_some_and(|t| !t.text.is_empty()) {
                    let focused = d.typed.get(&k).is_some_and(|t| t.focused);
                    let e = d.typed_mut(&form_id, &field_key);
                    *e = TextEdit::new("");
                    e.focused = focused;
                }
                d.typed_mut(&form_id, &field_key)
            })
            .on_focus_change(|s: &mut S, focused: bool, _tx: &mut Tx| {
                if focused {
                    s.composer_edit_mut().focused = false;
                    if let Some(f) = asking(s) {
                        dock(s, &f).active_option = None;
                    }
                }
            })
            .on_input_change(|s: &mut S, _| {
                let Some(f) = asking(s) else { return };
                let (tab, _) = place(s, &f);
                let k = f.key(tab);
                let d = s.dock_mut();
                d.custom.insert(k.clone());
                d.active_option = None;
                if !f.multi(tab) {
                    d.picks.remove(&k);
                }
            })
            .on_input_key(|s: &mut S, ev, tx| typed_key(s, ev, tx));
    } else {
        input = input.disabled(true).focusable(false);
    }
    let input = crate::scroller::scroller(format!("q-input-scroll-{tab}"))
        .build::<S>(p, col().grow(1.0).min_w(0.0).max_h(FIELD_MAX_H), input);
    let input_wrap = row().key("q-field").grow(1.0).shrink().min_w(0.0).min_h(28.0).pl(8.0).py(4.0).child(input);

    let last = f.last(tab);
    let submit = submit_hint(s.dock_question_submit());
    let advance_label = if last { opencode_i18n::t!("ui.promptInput.send") } else { opencode_i18n::t!("ui.common.next") };
    let advance_btn = composer_advance_button::<S>(p, last, advance_label, &submit, live);

    let mut footer = row()
        .key("question-footer")
        .w_full()
        .mt(8.0)
        .padding(8.0)
        .gap(4.0)
        .items_end()
        .radius(12.0)
        .fill(if p.dark { p.layer1 } else { p.bg_base })
        .elevate(p.raised)
        .transition(fade())
        .child(input_wrap)
        .child(row().key("q-actions").child(advance_btn));
    if live {
        // Clicking empty space on the composer card (padding, gutter) focuses `q-input` at the
        // end, matching the main session composer (`composer/mod.rs:367-379`).
        footer = footer
            .preserve_focus_on_press(true)
            .press_on_down()
            .on_press_tx(|s: &mut S, tx: &mut Tx| {
                let Some(f) = asking(s) else { return };
                let (tab, _) = place(s, &f);
                let k = f.key(tab);
                dock(s, &f).active_option = None;
                s.composer_edit_mut().focused = false;
                let d = s.dock_mut();
                if !d.custom_on(&k) && d.typed.get(&k).is_some_and(|t| !t.text.is_empty()) {
                    d.typed.remove(&k);
                }
                let e = d.typed_mut(&f.id, &f.fields[tab].key);
                if !e.focused {
                    let end = e.text.len();
                    e.set_selection(end, end);
                    e.focused = true;
                }
                tx.focus("q-input", false);
            });
    }

    Some(vec![body, footer])
}

/// Option row styling in composer mode (`question-answer-composer.css:50-137`): borderless,
/// flat highlight on `layer-01` / `layer-02`, and adjacent picked rows round outer corners only.
#[allow(clippy::too_many_arguments)]
fn option_row(p: &Pal, picked: bool, active: bool, prev_picked: bool, next_picked: bool, keyboard: bool, single_line: bool) -> El {
    let ease = feedback(keyboard);
    let top_r = if picked && prev_picked { 0.0 } else { 6.0 };
    let bot_r = if picked && next_picked { 0.0 } else { 6.0 };
    let bg = match (picked, active) {
        (true, true) => flat(p.hover, p.layer2),
        (true, false) => p.layer2,
        (false, true) => p.layer1,
        (false, false) => Color::TRANSPARENT,
    };
    let r = row()
        .w_full()
        .min_h(34.0)
        .px(10.0)
        .py(if single_line { 6.0 } else { 7.0 })
        .gap(10.0)
        .corners([top_r, top_r, bot_r, bot_r])
        .fill(bg)
        .motion(Spring::NONE)
        .hover_motion(ease, ease);
    if single_line { r.center_y() } else { r.items_start() }
}

/// Unpicked option mark uses `bg-layer-01` in composer mode (`question-answer-composer.css:87-89`).
fn option_mark(p: &Pal, multi: bool, picked: bool, keyboard: bool) -> El {
    if picked {
        return classic::mark(p, multi, true, keyboard);
    }
    let b = el()
        .key("box")
        .size(16.0, 16.0)
        .center()
        .radius(if multi { 4.0 } else { 8.0 })
        .fill(p.layer1)
        .box_shadow(Shadow { spread: 0.5, color: p.q.border_strong, inset: true, ..Shadow::default() });
    el().key("mark").size(16.0, 20.0).center().child(b)
}

/// Single icon-only advance button in the composer footer (`session-question-dock-classic.tsx:762-770`):
/// `arrow-up` submit button on the last question, `arrow-right` neutral button before it.
fn composer_advance_button<S: DockHost>(p: &Pal, last: bool, label: &'static str, submit_hint: &str, live: bool) -> El {
    let (fill, fg, hover, pressed) = if last {
        let (h, pr) = if p.dark {
            (Color::BLACK.alpha(0x14 as f32 / 255.0), p.v2.contrast_pressed)
        } else {
            (p.v2.contrast_hover, p.v2.contrast_pressed)
        };
        (p.q.submit, p.q.submit_text, h, pr)
    } else {
        let neutral = if p.dark && p.theme == 0 { flat(Color::WHITE.alpha(0.06), p.layer1) } else { p.q.neutral };
        (neutral, p.text, p.hover, p.pressed)
    };
    let glyph = if last {
        icon(&ic::ARROW_UP, 16.0).color(fg)
    } else {
        icon(&ic::ARROW_UP, 16.0).color(fg).rotate(std::f32::consts::FRAC_PI_2).mirror_in_rtl()
    };
    let mut b = el()
        .key("q-next")
        .role("button")
        .label(label)
        .size(28.0, 28.0)
        .radius(6.0)
        .center()
        .fill(fill)
        .hover_motion(Anim::INSTANT, Anim::INSTANT)
        .child(glyph);
    if live {
        b = b
            .focus_default(p)
            .help(format!("{label} {submit_hint}"))
            .help_above()
            .hover(hover)
            .press(pressed)
            .on_press_tx(|s: &mut S, tx: &mut Tx| {
                if let Some(f) = asking(s) {
                    next(s, &f, tx);
                }
            });
    } else {
        b = b.opacity(if last { 0.4 } else { 0.5 }).disabled(true).focusable(false);
    }
    if last {
        let lit = if p.dark { 0.6 } else { 0.2 };
        let (y, blur, drop) = if p.dark { (1.0, 2.0, 0.4) } else { (1.0, 1.5, 0.2) };
        b = b
            .gradient(Color::WHITE.alpha(lit), Color::WHITE.alpha(0.0))
            .box_shadow(Pal::ring_out(p.q.submit_ring))
            .shadow(y, blur, Color::BLACK.alpha(drop));
    } else {
        b = b.elevate(opencode_ui::toast::neutral_lift(p));
    }
    b
}

/// Keys while the composer-integrated question dock shows (`listKeys` + `nav`,
/// `session-question-dock-classic.tsx:346-465`).
pub fn key<S: DockHost>(s: &mut S, ev: &KeyEvent, tx: &mut Tx) -> bool {
    let Some(f) = asking(s) else { return false };
    let (tab, _) = place(s, &f);
    let editing_here = editing(s, &f, tab);
    if !editing_here && !focus_in_dock(tx.focused()) {
        return false;
    }
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
        enter_answer(s, &f, tab, tx);
        return true;
    }
    if editing_here || ev.mods.alt || ev.mods.ctrl || ev.mods.cmd || matches!(ev.key, Key::Command(_)) {
        return false;
    }
    let field = &f.fields[tab];
    let n = rows(field).len();
    if n == 0 || !focus_in_options(tx.focused()) {
        return false;
    }
    let active = active_option(s, &f, tab, n, false);
    match &ev.key {
        Key::Down => {
            tx.instant();
            match active {
                None => {
                    let d = dock(s, &f);
                    (d.focus, d.active_option, d.keyboard) = (0, Some(0), true);
                    reveal_option(tab, 0, tx);
                }
                Some(i) if i + 1 >= n => {
                    // Down from the last answer moves into the composer field (`session-question-dock-classic.tsx:422-427`).
                    let d = dock(s, &f);
                    (d.active_option, d.keyboard) = (None, true);
                    enter_field(s, &f, tab);
                    tx.focus("q-input", false);
                }
                Some(i) => {
                    let d = dock(s, &f);
                    (d.focus, d.active_option, d.keyboard) = (i + 1, Some(i + 1), true);
                    reveal_option(tab, i + 1, tx);
                }
            }
            return true;
        }
        Key::Up => {
            tx.instant();
            let to = match active {
                None | Some(0) => n - 1,
                Some(i) => i - 1,
            };
            let d = dock(s, &f);
            (d.focus, d.active_option, d.keyboard) = (to, Some(to), true);
            reveal_option(tab, to, tx);
            return true;
        }
        Key::Home => {
            tx.instant();
            let d = dock(s, &f);
            (d.focus, d.active_option, d.keyboard) = (0, Some(0), true);
            reveal_option(tab, 0, tx);
            return true;
        }
        Key::End => {
            tx.instant();
            let d = dock(s, &f);
            (d.focus, d.active_option, d.keyboard) = (n - 1, Some(n - 1), true);
            reveal_option(tab, n - 1, tx);
            return true;
        }
        _ => {}
    }
    let enter_kind = enter(ev, s.dock_question_submit());
    if enter_kind == Some(Enter::Submit) {
        if !tx.key_repeat() {
            dock(s, &f).keyboard = true;
            enter_answer(s, &f, tab, tx);
        }
        return true;
    }
    let digit = match &ev.key {
        Key::Text(t) if t.chars().count() == 1 => t.chars().next().and_then(|c| c.to_digit(10)).filter(|d| *d >= 1).map(|d| d as usize - 1),
        _ => None,
    };
    let pick_idx = if matches!(&ev.key, Key::Text(t) if t == " ") || enter_kind == Some(Enter::Select) {
        active
    } else {
        digit
    };
    let Some(i) = pick_idx.filter(|&i| i < n) else { return false };
    if !tx.key_repeat() {
        select(s, &f, tab, i);
        let d = dock(s, &f);
        (d.focus, d.active_option, d.keyboard) = (i, Some(i), true);
        reveal_option(tab, i, tx);
    }
    true
}

fn enter_answer<S: DockHost>(s: &mut S, f: &Form, tab: usize, tx: &mut Tx) {
    let n = rows(&f.fields[tab]).len();
    if let Some(i) = active_option(s, f, tab, n, editing(s, f, tab)) && !f.multi(tab) {
        select(s, f, tab, i);
    }
    next(s, f, tx);
}

/// Where a question lands after switching tabs in composer mode (`land()`,
/// `session-question-dock-classic.tsx:299-305,631`): in the composer field when there are no
/// preset options or a typed answer is already active, else on the options listbox.
pub(super) fn land<S: DockHost>(s: &mut S, f: &Form, tab: usize, tx: &mut Tx) {
    let n = rows(&f.fields[tab]).len();
    let k = f.key(tab);
    let has_typed = s.dock().custom_on(&k) && s.dock().typed.get(&k).is_some_and(|t| !t.text.is_empty());
    if n == 0 || has_typed {
        dock(s, f).active_option = None;
        enter_field(s, f, tab);
        tx.focus("q-input", false);
    } else {
        let rest = resting_option(s, f, tab).min(n - 1);
        let d = dock(s, f);
        d.focus = rest;
        d.active_option = Some(rest);
        reveal_option(tab, rest, tx);
    }
}

/// Keys in the composer custom-answer field (`session-question-dock-classic.tsx:594-644`):
/// Esc dismisses the question, Back/Advance work as anywhere, Up on the first line returns to
/// the last preset answer, the submit key advances, and everything else edits text.
fn typed_key<S: DockHost>(s: &mut S, ev: &KeyEvent, tx: &mut Tx) -> bool {
    let Some(f) = asking(s) else { return false };
    let (tab, _) = place(s, &f);
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
    if ev.mods.cmd || ev.mods.ctrl {
        return s.dock_shortcut(ev, tx);
    }
    let n = rows(&f.fields[tab]).len();
    if ev.key == Key::Up && !ev.mods.shift && !ev.mods.alt && n > 0 {
        let k = f.key(tab);
        let at_top = s.dock().typed.get(&k).is_none_or(|t| t.anchor == t.focus && t.text[..t.focus].rfind('\n').is_none());
        if at_top {
            stop_typing(s, &f, tab);
            let d = dock(s, &f);
            (d.focus, d.active_option, d.keyboard) = (n - 1, Some(n - 1), true);
            reveal_option(tab, n - 1, tx);
            return true;
        }
    }
    match enter(ev, s.dock_question_submit()) {
        Some(Enter::Submit) => {
            if !tx.key_repeat() {
                dock(s, &f).keyboard = true;
                next(s, &f, tx);
            }
            return true;
        }
        Some(Enter::Select) if !ev.mods.shift => {
            insert_typed_newline(s, &f, tab);
            return true;
        }
        _ => {}
    }
    false
}

/// Keeps the highlighted preset row visible inside the options scroller and moves keyboard focus
/// onto the listbox (`session-question-dock-classic.tsx:274-284`).
fn reveal_option(tab: usize, i: usize, tx: &mut Tx) {
    tx.focus(format!("q-options-{tab}"), false);
    tx.scroll_into_view_in(format!("q-scroll-{tab}"), format!("q-opt-{i}"));
}
