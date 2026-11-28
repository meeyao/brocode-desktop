//! Rendering for drawn menus (`menu-v2` and `appearance="standard"`), scrim placement, and static
//! preview surfaces.

use rune::{Align, El, PopupAnchor, Side, TextStyle, Tx, col, el, icon, row, text};
use crate::palette::Pal;
use crate::popover::{self, MENU, Opt};

use super::keys::key;
use super::menu::{Entry, Glyph, Look, Menu, hug_width, key_at, row_key};
use super::rows::{self, CHEVRON, detail_row, diff_note};
use super::search::{SEARCH_H, search_field};
use super::state::{choose, choose_at, close, hover_focus, left, moved, open_sub_at};
use super::{CARD, CONTEXT_MIN_W, GAP, GROUP, GROUP_H, PRESS_ON_DOWN, PopupHost, SELECT, SURFACE, focus, standard};

/// The drawn menu and the scrim under it that dismisses it, for the window's top layer.
pub fn view<S: PopupHost + 'static>(s: &S, p: &Pal) -> Vec<El> {
    let Some(shown) = &s.popup().shown else {
        s.popup().rows.borrow_mut().clear();
        return vec![];
    };
    {
        let mut rows = s.popup().rows.borrow_mut();
        rows.clear();
        let mut targets = s.popup().targets.borrow_mut();
        targets.clear();
        for (level, o) in shown.subs.iter().enumerate() {
            let Some(menu) = shown.menu_at(level) else { break };
            let (Some(trigger), Some(sub)) = (key_at(menu, o.at).and_then(|k| rune::memo::layout(&k)), shown.menu_at(level + 1)) else { break };
            let rects: Vec<[f32; 4]> = sub.entries.iter().filter_map(|e| match e {
                Entry::Item(i) => rune::memo::layout(&row_key(&i.id)),
                Entry::Sub(x) => rune::memo::layout(&row_key(&x.id)),
                _ => None,
            }).collect();
            let Some(first) = rects.first() else { break };
            let (left, right, top, bottom) = rects.iter().fold((first[0], first[0] + first[2], first[1], first[1] + first[3]), |(l, r, t, b), x| (l.min(x[0]), r.max(x[0] + x[2]), t.min(x[1]), b.max(x[1] + x[3])));
            let opens_right = left >= trigger[0];
            targets.push(super::submenu::Target { edge: if opens_right { left } else { right }, top, bottom, right: opens_right });
        }
        for level in 0..=shown.subs.len() {
            let Some(menu) = shown.menu_at(level) else { continue };
            for e in &menu.entries {
                if let Entry::Item(i) = e
                    && i.enabled
                    && let Some(r) = rune::memo::layout(&row_key(&i.id))
                {
                    rows.push((i.id.clone(), r));
                }
            }
        }
    }
    let over_modal = s.over_modal();
    // Desktop's context menu is not modal (Kobalte `ContextMenu`: `body` keeps `pointer-events: auto`), so
    // what the pointer rests on beside it stays hovered: a tab keeps its close button under its own menu.
    let scrim = scrim::<S>(!over_modal && matches!(shown.anchor, PopupAnchor::Point(..)) && !shown.menu.pointer_trigger);
    // The pointer's spot is physical, so it stays unmirrored in a right-to-left window.
    let rtl = opencode_i18n::direction() == opencode_i18n::Direction::Rtl;
    let (anchor, point, align, side) = match &shown.anchor {
        PopupAnchor::Key(key) => (key.clone(), None, shown.menu.align, shown.menu.side),
        &PopupAnchor::Point(x, y) if shown.menu.pointer_trigger => ("popup-point".to_owned(), Some(el().key("popup-point").absolute(x, y).unmirrored().size(1.0, 1.0)), Align::Start, Side::Bottom),
        // Kobalte's context menu opens `right-start` of the pointer, 2 off it and shifted 2 down
        // (`gutter: 2, shift: 2`, `context-menu-root.tsx`). Floating UI's `right` is physical in a
        // right-to-left page too, and Rune swaps an anchored side there, so ask for `Left`.
        &PopupAnchor::Point(x, y) => {
            let side = if rtl { Side::Left } else { Side::Right };
            ("popup-point".to_owned(), Some(el().key("popup-point").absolute(x, y + 2.0).unmirrored().size(0.0, 0.0)), Align::Start, side)
        }
    };
    let at_point = point.is_some();
    let gap = if at_point { GAP } else { shown.menu.gap.unwrap_or(4.0) };
    let mut layer = vec![scrim];
    layer.extend(point);
    if shown.menu.look == Look::Standard {
        let hair = s.hairline();
        let first = shown.subs.first().copied();
        layer.push(popover::place(s.knobs(), standard::root::<S>(p, &shown.menu, shown.active, first, hair), &anchor, side, align, gap));
        let sub = first.and_then(|o| Some((o, shown.menu.entries.get(o.at)?)));
        if let Some((o, Entry::Sub(sub))) = sub {
            layer.push(popover::place(s.knobs(), standard::sub::<S>(p, &sub.menu, o.active, hair), &row_key(&sub.id), Side::Right, Align::Start, 0.0));
        }
        return wrap_modal::<S>(over_modal, layer);
    }
    // A searchable menu lists what matches the field as it is now.
    let fresh = shown.search.as_ref().map(|search| {
        let query = &s.popup().query.text;
        let menu = (search.rows)(s, query);
        // Rows that changed since the menu last lit one light their first (`refresh`).
        let active = if menu.entries == shown.menu.entries { shown.active } else { menu.offered().first().copied() };
        (menu, active)
    });
    let (listed, active) = match &fresh {
        Some((menu, active)) => (menu, *active),
        None => (&shown.menu, shown.active),
    };
    let search_placeholder = shown.search.as_ref().map(|x| x.placeholder.as_str());
    // A right-click menu is at least Desktop's context-menu width; one from a control hugs its rows.
    let menu = drawn(s, p, listed, active, shown.subs.first().map(|o| o.at), search_placeholder);
    let menu = if at_point { menu.min_w(CONTEXT_MIN_W.max(shown.menu.min_w)) } else { menu };
    // `shift={-4}` is Floating UI's cross-axis offset, which it negates for a top or bottom
    // placement in a right-to-left page: the card then sits 4 past its trigger's right edge.
    let shift = if opencode_i18n::direction() == opencode_i18n::Direction::Rtl { 4.0 } else { -4.0 };
    let menu = if shown.menu.card && !at_point { menu.offset(shift, 0.0).offset_motion(rune::Anim::INSTANT) } else { menu };
    layer.push(popover::place(s.knobs(), menu, &anchor, side, align, gap));
    // Each open submenu beside its row, 4 off it (`Menu.Sub gutter={4}`) unless it says.
    let mut parent = listed;
    for (k, o) in shown.subs.iter().enumerate() {
        let Some(Entry::Sub(sub)) = parent.entries.get(o.at) else { break };
        let open = shown.subs.get(k + 1).map(|o| o.at);
        let gap = sub.menu.gap.unwrap_or(4.0);
        layer.push(popover::place(s.knobs(), build(Some(s), p, &sub.menu, o.active, open, k + 1, None), &row_key(&sub.id), Side::Right, Align::Start, gap));
        parent = &sub.menu;
    }
    wrap_modal::<S>(over_modal, layer)
}

/// Wraps `layer` in a raised modal scope when another Rune modal is mounted beneath it, so the
/// drawn menu, its submenus, and its outside-dismiss scrim stay above the modal in both paint
/// order and Rune's active-modal hit/focus filter.
fn wrap_modal<S: PopupHost + 'static>(over_modal: bool, layer: Vec<El>) -> Vec<El> {
    if !over_modal {
        return layer;
    }
    vec![
        el().key("popup-modal")
            .role("none")
            .absolute(0.0, 0.0)
            .w_full()
            .h_full()
            .transition(rune::Transition::none())
            .modal_tx(false, |s: &mut S, tx: &mut Tx| close(s, None, focus::Reason::Keys, tx))
            .children(layer)
            .raise(),
    ]
}

/// `menu` as a drawn surface, row `active` lit. Its rows light and pick the open menu's, so a
/// copy drawn while none is open (a storybook preview) stays still.
pub fn surface<S: PopupHost + 'static>(p: &Pal, menu: &Menu, active: Option<usize>) -> El {
    build::<S>(None, p, menu, active, None, 0, None)
}

/// [`surface`] with the host's project avatars, optional search field, and open submenu row.
fn drawn<S: PopupHost + 'static>(s: &S, p: &Pal, menu: &Menu, active: Option<usize>, open: Option<usize>, search_placeholder: Option<&str>) -> El {
    build(Some(s), p, menu, active, open, 0, search_placeholder)
}

/// A menu's surface: the root's (`level` 0), or the submenu open `level` deep, whose rows light,
/// pick and open their own submenus in it.
fn build<S: PopupHost + 'static>(
    s: Option<&S>,
    p: &Pal,
    menu: &Menu,
    active: Option<usize>,
    open: Option<usize>,
    level: usize,
    search_placeholder: Option<&str>,
) -> El {
    let nested = level > 0;
    // The first submenu keeps its keys; deeper ones number theirs.
    let name = |root: &str, sub: &str| match level {
        0 => root.to_owned(),
        1 => sub.to_owned(),
        _ => format!("{sub}-{level}"),
    };
    let mut list = popover::List::new(&name("popup-rows", "popup-sub-rows"), if menu.card { CARD } else if menu.select { SELECT } else { MENU });
    if let (Some(s), Some(placeholder)) = (s, search_placeholder) {
        list.other(search_field(s, p, placeholder), SEARCH_H);
    }
    let role = if menu.selected.is_some() { "menuitemradio" } else { "menuitem" };
    for (n, entry) in menu.entries.iter().enumerate() {
        match entry {
            Entry::Item(i) if i.detail => list.other(detail_row(p, &row_key(&i.id), i), MENU.row_h),
            Entry::Item(i) => {
                let o = Opt::new(row_key(&i.id), i.title.clone())
                    .role(if i.radio { "menuitemradio" } else { role })
                    .active(active == Some(n))
                    .checked(i.checked)
                    .danger(i.danger)
                    .disabled(!i.enabled)
                    .medium(i.medium);
                let ink = if i.danger { p.danger } else { p.text };
                // Menu items draw Desktop's `<Icon>` at its default `size="normal"` (16 pt, `icon.tsx:59`).
                let o = match i.glyph {
                    Some(Glyph::Ink(g)) if menu.card => o.lead(icon(g, 16.0).color(p.icon_muted)),
                    Some(Glyph::Ink(g)) => o.lead(icon(g, 16.0).color(ink)),
                    Some(Glyph::Weak(g)) => o.lead(icon(g, 14.0).color(crate::palette::token(p.dark, "icon-weak-base").unwrap_or(p.icon_faint))),
                    Some(Glyph::App(id)) => match rune_theme_opencode::app_icons::named(id, p.dark) {
                        Some(g) => o.lead(icon(g, 16.0)),
                        None => o.lead(el().size(16.0, 16.0)),
                    },
                    Some(Glyph::Slot(g)) => o.lead(row().size(16.0, 16.0).center().child(icon(g, 16.0).color(p.icon_muted))),
                    Some(Glyph::Status(ind)) => {
                        let mark = match s {
                            Some(host) => host.status_indicator(p, ind),
                            None => rows::status_mark(p, ind),
                        };
                        o.lead(row().size(16.0, 16.0).center().child(mark))
                    }
                    Some(Glyph::Swatch(light, dark)) => {
                        let [fill, edge] = if p.dark { dark } else { light };
                        o.lead(el().size(10.0, 10.0).radius(2.0).fill(fill).box_shadow(Pal::ring_in(edge)).a11y_hidden())
                    }
                    None => o,
                };
                let o = match &i.shortcut {
                    _ if !i.keycaps.is_empty() => o.trailing(crate::keymap::view::chips(p, i.keycaps.clone())),
                    Some(keys) => o.trailing(popover::hint(p, keys)),
                    None => o,
                };
                let o = match &i.hint {
                    // `ServerLabel`'s `gap-1`: 4 from the name.
                    Some(hint) => o.after(text(format!("· {hint}")).font(TextStyle::new(13.0, 440.0).lh(16.0).tracking(-0.04)).color(p.muted).ml(4.0 - MENU.gap)),
                    None => o,
                };
                // A status word sits at the words' end, before the check (`settings-server-switcher-status`).
                let o = match &i.note {
                    Some(note) => o.after(el().grow(1.0)).after(text(note.clone()).font(TextStyle::new(13.0, 440.0).lh(16.0).tracking(-0.04)).color(p.faint)),
                    None => o,
                };
                // A server row keeps its check's column when unchecked, as a radio item's indicator does.
                let o = if matches!(i.glyph, Some(Glyph::Status(_))) && !i.checked { o.trailing(el().size(16.0, 16.0)) } else { o };
                let look = if menu.card { CARD } else if menu.select { SELECT } else { popover::Look { pad_l: menu.item_inset.unwrap_or(MENU.pad_l), pad_r: menu.item_end.unwrap_or(MENU.pad_r), ..MENU } };
                let o = match i.diff {
                    Some((a, d)) => o.after(diff_note(p, a, d)),
                    None => o,
                };
                let o = match (s, &i.project) {
                    (Some(host), Some(dir)) => match host.project_badge(p, dir) {
                        Some(badge) => o.lead(badge),
                        None => o,
                    },
                    _ => o,
                };
                let row = if nested {
                    popover::option_tx::<S>(p, look, o, move |s, tx| hover_focus(s, level, n, tx), move |s, tx| choose_at(s, level, n, tx))
                } else {
                    popover::option_tx::<S>(p, look, o, move |s, tx| hover_focus(s, 0, n, tx), move |s, tx| choose(s, n, tx))
                };
                list.row(if i.enabled { takes_focus(row) } else { row });
            }
            Entry::Separator => {
                // A 1 pt rule across the whole surface, 2 above and below (`menu.css:163-169`):
                // stretched past the inset on both sides, as `width: calc(100% + 4px)` is.
                // In the location card the rows' own 2 pt gaps are that room (rows 33 apart across
                // it), and the line is `border-base` scaled to a hairline in its 1 pt box.
                let line = if menu.card {
                    // `session-identity-header.css:19-25` (`4d4a95a3239c`): `width: calc(100% + 4px);
                    // margin: 0 -2px; background: var(--v2-border-border-base); transform: scaleY(0.5);`
                    rune::hairline(p.border).key(format!("popup-sep-{n}")).mx(-MENU.inset)
                } else {
                    col().key(format!("popup-sep-{n}")).py(2.0).mx(-MENU.inset).child(el().key("line").h(1.0).w_full().fill(p.border_muted))
                };
                list.other(line, if menu.card { 1.0 } else { 5.0 });
            }
            Entry::Group(title) => {
                let label = row()
                    .key(format!("popup-group-{n}"))
                    .w_full()
                    .h(GROUP_H)
                    .pl(MENU.pad_l)
                    .pr(MENU.pad_r)
                    .center_y()
                    .child(text(title.clone()).font(GROUP).color(p.faint).shrink().truncate_fade());
                list.other(label, GROUP_H);
            }
            // A row that opens its menu beside it (`Menu.SubTrigger`), lit while that menu shows,
            // its chevron in the muted ink, at any depth.
            Entry::Sub(sub) => {
                let o = Opt::new(row_key(&sub.id), sub.title.clone())
                    .role("menuitem")
                    .active(sub.enabled && (active == Some(n) || open == Some(n)))
                    .disabled(!sub.enabled)
                    .middle_split(sub.split_at)
                    .trailing(icon(&CHEVRON, 16.0).key("chevron").color(p.icon_muted).mirror_in_rtl());
                let o = match sub.glyph.and_then(Glyph::mono) {
                    Some(g) => o.lead(icon(g, 16.0).color(p.icon_muted)),
                    None => o,
                };
                let look = if menu.card { CARD } else if menu.select { SELECT } else { popover::Look { pad_r: menu.item_end.unwrap_or(MENU.pad_r), ..MENU } };
                let row = popover::option_tx::<S>(
                    p,
                    look,
                    o,
                    move |s, tx| hover_focus(s, level, n, tx),
                    move |s, tx| open_sub_at(s, level, n, false, tx),
                );
                list.row(if sub.enabled { takes_focus(row) } else { row });
            }
        }
    }
    let surface = popover::surface::<S>(p, MENU, &name(SURFACE, "popup-submenu"), menu.label.clone())
        .min_w(menu.min_w)
        .on_pointer_move_local(|s: &mut S, e: rune::PointerEvent, tx: &mut Tx| moved(s, [e.window_x, e.window_y], tx));
    // An open menu holds focus (`menu-v2-content`, tabindex -1) and takes the keys from there.
    // A menu under an editing field holds it too ([`super::open_with_field`]); a search field keeps it.
    let holds_focus = search_placeholder.is_none() || s.is_some_and(|s| s.popup().holds_focus());
    let surface = match s {
        Some(_) if holds_focus => surface.focusable(true).tab_index(-1).no_focus_ring().on_key(|s: &mut S, ev, tx: &mut Tx| key(s, ev, tx)),
        _ => surface,
    };
    // A menu under an editing field is a Kobalte menu: the row the pointer lit goes dark as it leaves.
    let surface = if !nested && s.is_some_and(|s| s.popup().edits()) { surface.on_hover_change(|s: &mut S, inside, tx: &mut Tx| if !inside { left(s, tx) }) } else { surface };
    // A submenu is as wide as its widest row (`w-max`), its rows laid out apart from it.
    let width = menu.width.or_else(|| nested.then(|| hug_width(menu).max(menu.min_w)));
    let surface = match width {
        Some(w) => {
            let available = s.and_then(PopupHost::max_width).unwrap_or(w);
            let w = w.min(available);
            surface.min_w(menu.min_w.min(w)).w(w).max_w(w)
        }
        None => surface,
    };
    surface.child(list.build(&name("popup-scroll", "popup-sub-scroll"), menu.max_h))
}

/// A row of a menu that holds focus: Kobalte's items take it (tabindex -1), the lit one showing
/// it by its fill, not a ring.
pub(super) fn takes_focus(row: El) -> El {
    row.focusable(true).tab_index(-1).no_focus_ring()
}

/// A window-sized dismiss layer under the popup (`popover::scrim` generalized over [`PopupHost`]).
fn scrim<S: PopupHost + 'static>(hover_passes: bool) -> El {
    // A press outside focuses the scroll region it lands on ("scrollable content"), as one
    // outside Desktop's context menu does; a dropdown then hands focus back to its trigger.
    // Kobalte's menus dismiss on a press outside as the button goes down; a tap on the release.
    let base = el()
        .key("popup-scrim")
        .absolute(0.0, 0.0)
        .w_full()
        .h_full()
        .focusable(false)
        .on_press_tx(|s: &mut S, tx: &mut Tx| {
            popover::motion::closing(s.knobs(), tx);
            close(s, None, focus::Reason::Outside, tx);
        })
        .press_focuses_below()
        .on_secondary_press(|s: &mut S, _: rune::PointerPress, tx: &mut Tx| close(s, None, focus::Reason::Outside, tx));
    let base = if hover_passes { base.hover_passes_below() } else { base };
    if PRESS_ON_DOWN { base.press_on_down() } else { base }
}
