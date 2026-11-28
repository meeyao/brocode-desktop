//! Experimental › Sidebar sections, drawn: one list of side tabs, some under quiet section headers
//! ("Whisper headers": a label, no box) anywhere in it, a hairline under a section that a tab in no
//! section follows. With no sections it draws as the vertical `TabStrip` does (28 pt rows, 3 pt
//! apart). Selected tabs wear a soft accent wash, and while any are selected a small bar along the
//! sidebar's foot counts them and offers Group (⌘G) and Clear.

mod bar;
mod header;
mod menu;
mod row;
mod story;

use super::*;
use crate::tabs::sections::{self as sec, Entry, LIVE};
use opencode_i18n::t;

/// The space the selection bar takes at the list's foot.
const BAR_ROOM: f32 = bar::H + 8.0;
/// Top and bottom scroll-fade feather, matching vertical `TabStrip` (`TabOverflow::VERTICAL_DEFAULT`).
const FADE: f32 = 16.0;
/// How far the list's clip reaches into the sidebar's start padding: a header's triangle hangs
/// there, before its label.
const GUTTER: f32 = 8.0;
/// Half the 3 pt gap between rows (`TabStrip`'s vertical gap): each row's slot reaches it above
/// and below, so the list starts this far up.
pub(super) const REACH: f32 = 1.5;
/// Above a header that something stands over, and around the hairline under a section.
const HEAD_ABOVE: f32 = 12.0;
const LINE_AROUND: f32 = 6.0;

/// What the list moves on: a tab gliding to its new slot (into a section, back out, past a
/// fold), the rows a fold or a header pushes, and a pick's wash. Critical, never bouncy.
pub(super) const GLIDE: Spring = Spring::new(0.26, 0.0);

/// A fold opening or shutting: pops out of layout immediately on collapse so sibling blocks glide
/// on `GLIDE` without a held phantom box.
pub(super) fn fold() -> rune::Transition {
    let mut t = rune::Transition::reveal(rune::Edge::Top).springs(rune::SpringPair::new(GLIDE, Spring::new(0.22, 0.0)));
    let wipe = Some(rune::Reveal { edge: rune::Edge::Top, feather: 10.0 });
    (t.insertion.reveal, t.removal.reveal) = (wipe, wipe);
    t.combined(rune::Transition::opacity())
}

/// The side tabs in sections, with `lead` (Consolidated's pinned area) above them.
pub(super) fn list(s: &App, p: &Pal, lead: Option<El>) -> El {
    let laid = sec::laid(s);
    let st = sec::state(s);
    let nav_order = s.nav_order();
    let show_hints = crate::tab_hints::shown(s);
    let mut items = col().key("sidebar-sections-items").w_full().mt(-REACH).on_layout_tx(|s: &mut App, r: [f32; 4], tx: &mut Tx| {
        tx.live_only();
        s.kernel.slots.untracked(&LIVE).rects.borrow_mut().insert("content".into(), r);
    });
    // What the last entry left below its own ground: a row's slot reaches `REACH` past it.
    let mut below: Option<f32> = None;
    for (n, entry) in laid.entries.iter().enumerate() {
        match entry {
            Entry::Tab(i) => {
                if let Some(t) = s.tabs.get(*i) {
                    items = items.child(row::tab_row(s, p, t, *i, &nav_order, show_hints, false));
                }
                below = Some(REACH);
            }
            Entry::Section(section, tabs) => {
                // 12 pt over a header, less what stands above it already; none over the first.
                let above = below.map_or(REACH, |b| HEAD_ABOVE - b);
                let carried_sec = st.drag.carried_section(&section.id);
                let landing_sec = !st.drag.is_gesture() && section.members.first().is_some_and(|u| st.drag.landing(u));
                let raised_sec = carried_sec.is_some() || landing_sec;
                let created = st.naming.is_created(&section.id);
                let mut block = col()
                    .key(format!("sec-block-{}", section.id))
                    .w_full()
                    .mt(above)
                    .radius(6.0)
                    .fill(p.bg_deep)
                    .motion(GLIDE)
                    .child(header::header(s, p, section, tabs));
                below = Some(0.0);
                if !section.collapsed {
                    let rows = tabs.iter().filter_map(|&i| s.tabs.get(i).map(|t| row::tab_row(s, p, t, i, &nav_order, show_hints, created)));
                    let group = col()
                        .key(format!("sec-rows-{}", section.id))
                        .w_full()
                        .pt(REACH)
                        .children(rows)
                        .motion(GLIDE)
                        .transition(if created { rune::Transition::none() } else { fold() });
                    block = block.child(group);
                    below = Some(REACH);
                }
                block = match carried_sec {
                    Some(dy) => block.radius(6.0).elevate(p.floating).raise().offset(0.0, dy).offset_motion(rune::Anim::INSTANT),
                    None => {
                        let b = block.offset(0.0, st.drag.block_shift(&section.id));
                        if raised_sec { b.raise() } else { b }
                    }
                };
                items = items.child(block);
                if matches!(laid.entries.get(n + 1), Some(Entry::Tab(_))) {
                    let hidden = st.drag.line_hidden(&section.id);
                    let line = el()
                        .key(format!("sec-line-{}", section.id))
                        .mx(8.0)
                        .h(1.0)
                        .fill(p.border_muted)
                        .mt(LINE_AROUND - below.unwrap_or(0.0))
                        .mb(LINE_AROUND - REACH)
                        .motion(GLIDE)
                        .offset(0.0, st.drag.line_shift(&section.id))
                        .opacity(if hidden { 0.0 } else { 1.0 })
                        .transition(rune::Transition::opacity())
                        .a11y_hidden();
                    items = items.child(line);
                    below = None;
                }
            }
        }
    }
    if let Some(shifts) = st.drag.shifts() {
        for (id, ly) in &shifts.preview_lines {
            items = items.child(
                el().key(format!("sec-preview-line-{id}"))
                    .pin(Some(8.0), Some(*ly), Some(8.0), None)
                    .h(1.0)
                    .fill(p.border_muted)
                    .transition(rune::Transition::opacity())
                    .a11y_hidden(),
            );
        }
        if let Some(ey) = shifts.end_line {
            items = items.child(
                el().key("sections-end-drop")
                    .pin(Some(8.0), Some(ey - 1.0), Some(8.0), None)
                    .h(2.0)
                    .radius(1.0)
                    .fill(p.accent.alpha(0.8))
                    .raise()
                    .a11y_hidden(),
            );
        }
    }
    let picked = !st.selection.is_empty();
    let content = col()
        .key("sidebar-sections")
        .role("tablist")
        .label_static(t!("settings.general.row.interfaceMode.side"))
        .focus_group()
        .w_full()
        .pb(if picked { BAR_ROOM } else { FADE })
        .motion(GLIDE)
        .children(lead)
        .child(items)
        .on_layout_tx(|s: &mut App, r: [f32; 4], tx: &mut Tx| {
            tx.live_only();
            s.strip_laid_out(r);
        });
    let live = s.kernel.slots.untracked(&LIVE);
    let scroll_pos = live.scroll_pos.clone();
    let viewport = col()
        .grow(1.0)
        .shrink()
        .min_h(0.0)
        .ml(-GUTTER)
        .focus_inset(p)
        .scroll_fade(rune::ScrollFade::feather(FADE))
        .scroll_margin(FADE)
        .drawn_rect_into(&live.scroll_rect)
        .on_scroll_offset(move |at| scroll_pos.set(at));
    let scroll = session_view::scroller::scroller(sec::SCROLL_KEY)
        .bar(session_view::scroller::Bar::Hidden)
        .build::<App>(p, viewport, col().key("gutter").w_full().pl(GUTTER).child(content));
    col().key("sidebar-sections-list").w_full().grow(1.0).shrink().min_h(0.0).child(scroll).children(picked.then(|| bar::bar(s, p)))
}
