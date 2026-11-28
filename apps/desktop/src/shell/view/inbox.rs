use super::*;
use std::rc::Rc;

use crate::inbox_model::{Entry, InboxStatus, age};
use crate::inbox_order::{Folds, Group, Order};
use crate::popup::{self, Item, Menu};
use crate::restore::{InboxUi, WorkingPlacement};
use crate::tabs::pins::{self, PinRef};
use opencode_i18n::t;

// ---------------------------------------------------------------------------------------------
// Inbox: Desktop's Inbox layout in the sidebar (`shell/titlebar/inbox.tsx`): Drafts, Needs
// attention, the pinned and the recent sessions and Archived in a scrolling list, and Working
// on a shelf below it or in the list above Archived.

/// A card's title (`text-[13px] leading-4 font-medium tracking-[-0.04px]`).
const TITLE: TextStyle = TextStyle::new(13.0, 500.0).lh(16.0).tracking(-0.04);
/// The project line under it, and a group's header (`text-[13px] leading-4`).
const LINE: TextStyle = TextStyle::new(13.0, 440.0).lh(16.0).tracking(-0.04);
const HEADER: TextStyle = TextStyle::new(13.0, 500.0).lh(16.0);
/// The age (`text-[12px] tabular-nums`).
const AGE: TextStyle = TextStyle::new(12.0, 440.0).lh(16.0).tracking(-0.04).features("\"tnum\"");
/// Show more and Archived: a ghost-muted Button (`button.css`), 13/20, 440.
const BUTTON: TextStyle = TextStyle::new(13.0, 440.0).lh(20.0).tracking(-0.04).features("\"tnum\"");
/// The chevron's turn (`transition-transform`: 150 ms, Tailwind's default ease).
const TURN: Curve = Curve::tween(0.15, Easing::cubic(0.4, 0.0, 0.2, 1.0));

/// The inbox in the sidebar's list slot. `inset` is the sidebar's start and end padding, which
/// the Working shelf's hairline bleeds across to the panel's edge.
pub(crate) fn inbox(s: &App, p: &Pal, inset: (f32, f32)) -> El {
    if s.sidebar_inbox.loading() {
        // Until the first load settles: Desktop's `Loader`, centred 8 in (`flex justify-center p-2`).
        return col().key("sidebar-inbox").role("list").label_static(t!("settings.general.row.interfaceMode.inbox")).w_full().grow(1.0).shrink().min_h(0.0).child(
            row().key("inbox-loading").w_full().padding(8.0).justify_center().child(rune_kit::Loader::new().build(crate::palette::kit(p))),
        );
    }
    let order = s.inbox_order();
    let ui = s.kernel.slots.get(&crate::restore::RESTORE).inbox;
    let drafts = Rc::new(draft_ids(s));
    let items = Rc::new(items(&order, ui, drafts.len()));
    // Only the rows in sight are built (a virtual list), whatever Show more shows: each row is
    // its session, and its card reads the session as it stands. The rows' places stand as the
    // groups' gaps did: 2 between cards, 8 under a header, 10 under a group and 2 between groups.
    let n = items.len();
    // The last group's own 10 of padding under it, then the list's 4.
    let pb = if matches!(items.last(), Some(Line::Card { .. } | Line::Draft { .. } | Line::More | Line::Toggle(_))) { 14.0 } else { 4.0 };
    let version = (order.generation, ui, drafts.as_slice().to_vec());
    let (keys, built, pal) = (items.clone(), items.clone(), *p);
    let (key_order, built_order) = (order.clone(), order.clone());
    let (key_drafts, built_drafts) = (drafts.clone(), drafts.clone());
    let epoch = epoch(&items, &order);
    let built_epoch = epoch.clone();
    let list = col()
        .key("content")
        .w_full()
        .pb(pb)
        .growth(rune::GrowthSpec::default())
        .virtual_rows(
            n,
            // Most rows are a card and the 2 above it.
            CARD_H + 2.0,
            move |_: &App, i| keys[i].key(&key_order, &key_drafts, &epoch).into(),
            move |s: &App, i| {
                let item = &built[i];
                el().w_full().pt(space_above(&built, i)).child(item.build(s, &pal, &built_order, &built_drafts, &built_epoch))
            },
        )
        .virtual_rows_version(version);
    col()
        .key("sidebar-inbox")
        .role("list")
        .label_static(t!("settings.general.row.interfaceMode.inbox"))
        .focus_group()
        .w_full()
        .grow(1.0)
        .shrink()
        .min_h(0.0)
        // Home and End go to the inbox's first and last rows, built or not: the arrows' own Home
        // and End reach only the rows built. A row not built is scrolled to, which builds it.
        .on_key(|s: &mut App, ev: &rune::KeyEvent, tx: &mut Tx| {
            if ev.mods != rune::Mods::default() || !matches!(ev.key, rune::Key::Home | rune::Key::End) {
                return false;
            }
            let Some((row, key)) = end_key(s, ev.key == rune::Key::End) else { return false };
            // Focus goes on once the scroll has built its row: the next poll (`inbox_model::poll`).
            tx.scroll_into_view(row);
            s.sidebar_inbox.focus_next = Some((key, true));
            tx.wake_after(std::time::Duration::ZERO);
            true
        })
        .on_layout_tx(|s: &mut App, r: [f32; 4], tx: &mut Tx| {
            if (s.sidebar_inbox.height - r[3]).abs() > 0.5 {
                s.sidebar_inbox.height = r[3];
            } else {
                tx.live_only();
            }
        })
        // The list's own menu, opened anywhere outside a card (`InboxSessionList`'s `menu`).
        .on_secondary_press(|s: &mut App, press: rune::PointerPress, tx: &mut Tx| open_list_menu(s, press, tx))
        .child(scroller(p, "inbox-scroll", list).grow(1.0))
        .children((ui.working_placement == WorkingPlacement::Shelf && !order.group(Group::Working).is_empty()).then(|| working(s, p, &order, inset)))
}

/// The list's menu: where Working sessions go (`sidebar.inbox.working.placement`).
fn open_list_menu(s: &mut App, at: rune::PointerPress, tx: &mut Tx) {
    let placed = s.kernel.slots.get(&crate::restore::RESTORE).inbox.working_placement;
    let option = |at: WorkingPlacement, label: &str| Item::new(at.name(), label).radio(placed == at);
    let places = Menu::new(t!("sidebar.inbox.working.placement"))
        .item(option(WorkingPlacement::Shelf, t!("sidebar.inbox.working.shelf")))
        .item(option(WorkingPlacement::Inline, t!("sidebar.inbox.working.inline")));
    let menu = Menu::new(t!("settings.general.row.interfaceMode.inbox")).submenu("working", t!("sidebar.inbox.working.placement"), places);
    tx.instant();
    popup::open(s, menu, at, tx, |s: &mut App, picked, tx| {
        let at = match picked {
            Some("shelf") => WorkingPlacement::Shelf,
            Some("inline") => WorkingPlacement::Inline,
            _ => return,
        };
        tx.instant();
        s.place_inbox_working(at);
    });
}

/// A card's height (`InboxSessionCard`, 54pt): the list's estimate for rows it hasn't laid out.
const CARD_H: f32 = 54.0;

/// One row of the inbox's list.
#[derive(Clone, Copy, Debug)]
enum Line {
    Header(Group),
    /// Drafts' header (`sidebar.inbox.section.drafts`).
    Drafts,
    /// The draft tab `drafts[at]`, `pos` of `size`.
    Draft { at: usize, pos: usize, size: usize },
    /// The card of `order.rows[at]` in group `g`, `pos` of `size` there (A11Y: true for rows not built).
    Card { g: Group, at: usize, pos: usize, size: usize },
    /// Show more / Show less under Recent.
    More,
    Empty,
    /// The header of a group that folds: Working in the list, and Archived.
    Toggle(Group),
}

/// The list's rows in order: `drafts` drafts under their header, the open session's card when no
/// group shows it, Needs attention under its header, the pinned and the recent sessions under the
/// Recent header (`opencode#66494`, with Show more), Working when it isn't the shelf (peeking the
/// selected session when collapsed, `opencode#66500`), Archived under its toggle, and the empty line.
fn items(order: &Order, ui: InboxUi, drafts: usize) -> Vec<Line> {
    let mut out = Vec::new();
    if drafts > 0 {
        out.push(Line::Drafts);
        out.extend((0..drafts).map(|at| Line::Draft { at, pos: at + 1, size: drafts }));
    }
    let card = |out: &mut Vec<Line>, g: Group, shows: &[usize]| {
        let size = shows.len();
        out.extend(shows.iter().enumerate().map(|(k, &at)| Line::Card { g, at, pos: k + 1, size }));
    };
    let folds = Folds::from(ui);
    card(&mut out, Group::Current, &order.shows(Group::Current, folds));
    let needs = order.shows(Group::Needs, folds);
    if !needs.is_empty() {
        out.push(Line::Header(Group::Needs));
        card(&mut out, Group::Needs, &needs);
    }
    // Recent shows its visible section header (`inbox.tsx:414-420`, `opencode#66494`).
    let recent = order.shows(Group::Recent, folds);
    if !recent.is_empty() {
        out.push(Line::Header(Group::Recent));
        card(&mut out, Group::Recent, &recent);
        if order.more {
            out.push(Line::More);
        }
    }
    // Between the active and the archived sessions when Working is not the bottom shelf; collapsed,
    // still shows the selected working session (`inbox.tsx:440-450`, `opencode#66500`).
    if ui.working_placement == WorkingPlacement::Inline && !order.group(Group::Working).is_empty() {
        out.push(Line::Toggle(Group::Working));
        let shown = order.shows(Group::Working, folds);
        if !shown.is_empty() {
            card(&mut out, Group::Working, &shown);
        }
    }
    if !order.group(Group::Archived).is_empty() {
        out.push(Line::Toggle(Group::Archived));
        card(&mut out, Group::Archived, &order.shows(Group::Archived, folds));
    }
    if order.is_empty() && drafts == 0 {
        out.push(Line::Empty);
    }
    out
}
/// What the list's row keys carry besides their element: the open session while it stands
/// first as the unheaded current row. A virtual list holds the reader's place when rows come in
/// above the ones in view, so that row coming in at the top of a list at its top would push
/// itself out of sight, where Desktop's list (a page at `scrollTop` 0) shows it. With every
/// row's key new, there's no place to hold and the list stays where it is.
fn epoch(items: &[Line], order: &Order) -> String {
    items.iter().find_map(|l| match l {
        Line::Card { g: Group::Current, at, .. } => Some(order.rows[*at].id.clone()),
        _ => None,
    }).unwrap_or_default()
}

/// The inbox's first or (`last`) last row a key can reach, in the list or, open, the Working
/// shelf below it: the row to scroll to (the list's own key for one of its rows), and the
/// element to focus.
fn end_key(s: &App, last: bool) -> Option<(String, String)> {
    let order = s.inbox_order();
    let drafts = draft_ids(s);
    let ui = s.kernel.slots.get(&crate::restore::RESTORE).inbox;
    let items = items(&order, ui, drafts.len());
    let epoch = epoch(&items, &order);
    let reach = |l: &Line| match l {
        Line::Card { at, .. } => Some(order.rows[*at].key.clone()),
        Line::Draft { .. } => Some(l.element_key(&order, &drafts)),
        Line::More => Some("inbox-more".to_owned()),
        Line::Header(_) | Line::Drafts | Line::Empty | Line::Toggle(_) => None,
    }
    .map(|key| (l.key(&order, &drafts, &epoch), key));
    let shelf: Vec<&crate::inbox_order::Shown> = if ui.working_placement == WorkingPlacement::Shelf {
        order.shows(Group::Working, Folds::from(ui)).into_iter().map(|at| &order.rows[at]).collect()
    } else {
        Vec::new()
    };
    let shelved = |r: &&crate::inbox_order::Shown| (r.key.clone(), r.key.clone());
    if last {
        shelf.last().map(shelved).or_else(|| items.iter().rev().find_map(reach))
    } else {
        items.iter().find_map(reach).or_else(|| shelf.first().map(shelved))
    }
}
/// Scrolls the inbox's row of the session keyed `key` (`Entry::key`) into view, just far enough
/// (`inbox.tsx` `navigate`'s `reveal`: `scrollIntoView({ block: "nearest" })`): a row of the
/// list by the list's own key for it, built or not, else the card on the Working shelf.
pub(crate) fn reveal(s: &App, key: &str, tx: &mut Tx) {
    let order = s.inbox_order();
    let drafts = draft_ids(s);
    let ui = s.kernel.slots.get(&crate::restore::RESTORE).inbox;
    let items = items(&order, ui, drafts.len());
    let epoch = epoch(&items, &order);
    let card = format!("ib-{key}");
    let row = items.iter().find(|l| matches!(l, Line::Card { at, .. } if order.rows[*at].key == card)).map(|l| l.key(&order, &drafts, &epoch));
    tx.scroll_into_view(row.unwrap_or(card));
}

/// The room above row `i`, as the groups' gaps and padding laid it out.
fn space_above(items: &[Line], i: usize) -> f32 {
    let Some(prev) = i.checked_sub(1).map(|j| items[j]) else { return 0.0 };
    match (prev, items[i]) {
        // Under a group: its 10 of padding and the list's 2. A group shut is its header alone.
        (Line::Card { .. } | Line::More | Line::Draft { .. } | Line::Toggle(_), Line::Header(_) | Line::Toggle(_)) => 12.0,
        // Drafts' group ends with its own 10 before the open session's card too.
        (Line::Draft { .. }, Line::Card { .. }) => 12.0,
        // Recent has no header: its first card stands a group's room under the cards above it.
        (Line::Card { g: above, .. }, Line::Card { g, .. }) if above != g => 12.0,
        (Line::Header(_) | Line::Drafts | Line::Toggle(_), _) | (_, Line::More) => 8.0,
        _ => 2.0,
    }
}

impl Line {
    /// The list's key for the row: `line-` and its element's, so a key names one element, and
    /// the list's [`epoch`] when it has one.
    fn key(&self, order: &Order, drafts: &[String], epoch: &str) -> String {
        if epoch.is_empty() {
            return format!("line-{}", self.element_key(order, drafts));
        }
        format!("line-{}@{epoch}", self.element_key(order, drafts))
    }

    fn element_key(&self, order: &Order, drafts: &[String]) -> String {
        match self {
            Line::Header(g) | Line::Toggle(g) => format!("inbox-{}-header", group_id(*g)),
            Line::Drafts => "inbox-drafts-header".into(),
            Line::Draft { at, .. } => format!("ib-{}", drafts[*at]),
            Line::Card { at, .. } => order.rows[*at].key.clone(),
            Line::More => "inbox-more".into(),
            Line::Empty => "inbox-empty".into(),
        }
    }

    fn build(&self, s: &App, p: &Pal, order: &Order, drafts: &[String], current_epoch: &str) -> El {
        match *self {
            Line::Header(g) => header(p, group_id(g), group_label(g)),
            Line::Drafts => header(p, "drafts", t!("sidebar.inbox.section.drafts")),
            Line::Draft { at, pos, size } => match s.tab(&drafts[at]) {
                Some(tab) => draft_card(s, p, tab).position_in_set(pos, size),
                None => el().key(self.element_key(order, drafts)).h(CARD_H),
            },
            Line::Toggle(Group::Archived) => {
                let open = s.kernel.slots.get(&crate::restore::RESTORE).inbox.archived_expanded;
                toggle_header(s, p, Group::Archived, open, false).on_press_tx(|s: &mut App, tx: &mut Tx| {
                    if !tx.by_keys() { s.sidebar_inbox.focus.note_pointer(); }
                    tx.instant();
                    s.toggle_inbox_archived();
                })
            }
            Line::Toggle(_) => working_header(s, p, s.kernel.slots.get(&crate::restore::RESTORE).inbox.working_expanded),
            Line::Card { g, at, pos, size } => match s.inbox_entry_of(&order.rows[at]) {
                Some(e) => card(s, p, &e, false, g, current_epoch).position_in_set(pos, size),
                // Gone since the rows were read: the next reading drops it.
                None => el().key(self.element_key(order, drafts)).h(CARD_H),
            },
            Line::More => {
                let label = if s.kernel.slots.get(&crate::restore::RESTORE).inbox.recent_expanded { t!("sidebar.inbox.showLess") } else { t!("sidebar.inbox.showMore") };
                // Focus stays on the button, which goes to the group's end as Show less, and
                // the list stays where it is (`inbox.tsx`: the same button, its label changed).
                ghost_button(p, "inbox-more", label).on_press_tx(|s: &mut App, tx: &mut Tx| {
                    if !tx.by_keys() { s.sidebar_inbox.focus.note_pointer(); }
                    tx.instant();
                    let opening = !s.kernel.slots.get(&crate::restore::RESTORE).inbox.recent_expanded;
                    s.toggle_inbox_recent();
                    // That holds while the button's new place is one the list still builds,
                    // about two list heights past the view. Further down the list drops the
                    // row, and its focus would land on the last card built, out of sight, for
                    // the next Return to open. So past a list height and a half of new rows,
                    // focus goes to the first row the press brought, where the button stood.
                    let order = s.inbox_order();
                    let first = order.pinned + crate::inbox_model::RECENT_LIMIT;
                    let brought = order.group(Group::Recent).len().saturating_sub(first);
                    let near = brought as f32 * (CARD_H + 2.0) <= 1.5 * s.sidebar_inbox.height;
                    if opening && !near && let Some(row) = order.group(Group::Recent).get(first) {
                        tx.focus(row.key.clone(), s.sidebar_inbox.focus.ring());
                    }
                })
            }
            // `px-1.5 py-2`, a block whose words wrap at the sidebar's width (`inbox.tsx:421`).
            // `inbox.tsx`: `text-[13px] leading-4 text-v2-text-text-faint` (no negative tracking).
            Line::Empty => col().key("inbox-empty").w_full().px(6.0).py(8.0).child(text(t!("sidebar.inbox.empty")).font(TextStyle::new(13.0, 440.0).lh(16.0)).color(p.faint).wrap().w_full()),
        }
    }
}

fn group_id(g: Group) -> &'static str {
    match g {
        Group::Current => "current",
        Group::Needs => "needs",
        Group::Recent => "recent",
        Group::Working => "working",
        Group::Archived => "archived",
    }
}

fn group_label(g: Group) -> &'static str {
    match g {
        Group::Needs => t!("sidebar.inbox.section.needsAttention"),
        Group::Working => t!("sidebar.inbox.section.working"),
        Group::Archived => t!("sidebar.inbox.section.archived"),
        Group::Current | Group::Recent => t!("sidebar.inbox.section.recent"),
    }
}

/// Which of the inbox's two scroll areas.
/// A plain `overflow-y-auto` area, with `scrollbar.css`'s bar in the gutter it takes while it overflows.
fn scroller(p: &Pal, key: &str, content: El) -> El {
    session_view::scroller::scroller(key).build::<App>(p, col().w_full().shrink().min_h(0.0), content)
}

/// Working sessions, in a shelf pinned below the list: at most 40% of the inbox, behind a
/// full-bleed hairline. Collapsed, still shows the selected working session (`opencode#66500`).
fn working(s: &App, p: &Pal, order: &Order, inset: (f32, f32)) -> El {
    let ui = s.kernel.slots.get(&crate::restore::RESTORE).inbox;
    let open = ui.working_expanded;
    let shown = order.shows(Group::Working, Folds::from(ui));
    let mut shelf = col()
        .key("inbox-working")
        .role("group")
        .label_static(t!("sidebar.inbox.section.working"))
        .w_full()
        .gap(8.0)
        .pt(7.0)
        // From 1pt into the sidebar to 1pt short of the panel, past the 8pt gutter (`--sidebar-divider-bleed`).
        .child(el().key("inbox-working-rule").absolute(1.0 - inset.0, 0.0).w(*s.sidebar_w - 2.0 + 8.0).h(1.0).fill(p.border_muted))
        .child(working_header(s, p, open));
    if s.sidebar_inbox.height > 0.0 {
        shelf = shelf.max_h(s.sidebar_inbox.height * 0.4);
    }
    if !shown.is_empty() {
        let rows = col()
            .key("content")
            .w_full()
            .gap(2.0)
            .children(shown.into_iter().filter_map(|at| s.inbox_entry_of(&order.rows[at])).map(|e| card(s, p, &e, true, Group::Working, "")));
        shelf = shelf.child(scroller(p, "inbox-working-rows", rows));
    }
    shelf
}

/// A group's 24pt header: 13/16 medium, muted.
fn header(p: &Pal, id: &str, label: &str) -> El {
    row().key(format!("inbox-{id}-header")).h(24.0).w_full().px(8.0).gap(6.0).center_y().child(text(label).font(HEADER).color(p.muted).truncate())
}

/// The Working header, on the shelf or in the list: its label shimmers while it's shut.
fn working_header(s: &App, p: &Pal, open: bool) -> El {
    toggle_header(s, p, Group::Working, open, !open).on_press_tx(|s: &mut App, tx: &mut Tx| {
        if !tx.by_keys() { s.sidebar_inbox.focus.note_pointer(); }
        tx.instant();
        s.toggle_inbox_working();
    })
}

/// The header of a group that folds (`SectionHeader` with `onToggle`): a toggle whose chevron
/// turns shut, and whose label shimmers when `shimmer`.
fn toggle_header(s: &App, p: &Pal, g: Group, open: bool, shimmer: bool) -> El {
    let label = if shimmer {
        session_view::working::shimmer(s, p, group_label(g), HEADER, 0.0)
    } else {
        text(group_label(g)).key("label").font(HEADER).color(p.muted).group_color(p.text)
    };
    row()
        .key(format!("inbox-{}-header", group_id(g)))
        .role("button")
        .label_static(group_label(g))
        .expanded(open)
        .h(24.0)
        .w_full()
        .px(8.0)
        .gap(6.0)
        .center_y()
        .radius(6.0)
        .focus_inset(p)
        .hover_group()
        .child(label)
        .child(
            icon(&ic::FILL_TRIANGLE_DOWN, 14.0)
                .key("chevron")
                .color(p.muted)
                .group_color(p.text)
                .rotate(if open { 0.0 } else { -std::f32::consts::FRAC_PI_2 }).mirror_in_rtl()
                .rotate_motion(TURN),
        )
}

/// A ghost-muted, full-width Button with its label at the start (`w-full justify-start px-2`).
fn ghost_button(p: &Pal, key: &str, label: &str) -> El {
    row()
        .key(key)
        .role("button")
        .label(label)
        .h(28.0)
        .w_full()
        .px(8.0)
        .gap(6.0)
        .center_y()
        .radius(6.0)
        .focus_inset(p)
        .hover(p.hover)
        .press(p.pressed)
        .child(text(label).font(BUTTON).color(p.muted))
}

/// `InboxSessionCard`: 54pt, two lines. The title, then the project, its worktree and branch
/// marks, and at the end the status and the age.
fn card(s: &App, p: &Pal, e: &Entry<'_>, in_shelf: bool, group: Group, card_epoch: &str) -> El {
    let info = e.session;
    let selected = s.inbox_selected(e);
    let project = e.project;
    let name = project_or_dir(project, &info.directory);
    let title = crate::tabs::tab_title(&info.title);
    let mut place = row()
        .key("place")
        .grow(1.0)
        .shrink()
        .min_w(0.0)
        .gap(8.0)
        .center_y()
        .clip_soft(0.0)
        .child(text(name.clone()).font(LINE).color(p.muted).shrink().min_w(0.0).truncate().dir(rune::Direction::Auto));
    if e.worktree {
        place = place.child(icon(&ic::OUTLINE_WORKTREE, 16.0).key("worktree").color(p.icon_muted).help(info.directory.clone()).help_side(rune::Side::Right));
    }
    if let Some(branch) = e.branch {
        place = place.child(icon(&ic::BRANCH, 16.0).key("branch").color(p.icon_muted).help(branch.to_owned()).help_side(rune::Side::Right));
    }
    // The session's server, where tabs of several servers mix (`servers.tag`): a 16 pt server
    // glyph at the line's start, under the avatar, naming it on hover (`inbox.tsx:617-623`).
    let server = crate::servers::runtime::tag(e.server).map(|name| {
        el().key("server").absolute(0.0, 0.0).size(16.0, 16.0).center().label(name.clone()).help(name).help_side(rune::Side::Right).child(icon(&SERVER, 16.0).color(p.icon_muted))
    });
    let mut end = row().key("end").gap(6.0).center_y();
    match e.status {
        InboxStatus::Ready => {}
        InboxStatus::Working => end = end.child(activity_ring(p)),
        status => {
            let fill = match status {
                InboxStatus::Approval | InboxStatus::Input => p.warning,
                InboxStatus::Failed => p.danger,
                _ => p.bg_accent,
            };
            end = end.child(el().key("dot").size(7.0, 7.0).radius(3.5).fill(fill));
        }
    }
    if let Some(label) = age(e.status, e.since, *s.now_ms) {
        end = end.child(text(label).key("age").font(AGE).color(p.faint));
    }
    let key = e.key();
    let (id, menu_id) = (key.clone(), key.clone());
    let archived = info.archived.is_some();
    // `inbox.tsx` `InboxSessionCard`: a pinned session's pin after its title, muted.
    let pin = pins::state(s).doc.is_pinned(&PinRef::new(e.server.clone(), info.id.clone())).then(|| icon(&PIN, 16.0).key("pin").color(p.icon_muted).label_static(t!("sidebar.recent.pinned")));
    let card_key = format!("ib-{key}");
    let follow_key = card_key.clone();
    let card_epoch = card_epoch.to_owned();
    let mut card = col()
        .key(card_key)
        .role("button")
        .label(title.clone())
        .w_full()
        .h(54.0)
        .padding(8.0)
        .gap(6.0)
        .radius(6.0)
        .focus_inset(p)
        .focusable(true)
        .focus_selected(selected)
        // Follow a focused session card across section/placement/epoch remounts, and rescue focus
        // to a surviving neighbour on removal or section collapse; show ring and scroll only when
        // driven by NAVIGATION keys (`focus-rescue.tsx:58-209`, `inbox.tsx:329`, `opencode#65690`, `opencode#66547`).
        .on_focus_change(move |s: &mut App, focused: bool, tx: &mut Tx| {
            if s.tab_layout() != crate::layout_state::TabLayout::Inbox {
                tx.live_only();
                return;
            }
            if focused {
                let order = crate::session::inbox::focus::visible_card_keys(s);
                let _ = crate::session::inbox::focus::step(
                    &mut s.sidebar_inbox.focus,
                    crate::session::inbox::focus::FocusEvent::Focused {
                        key: follow_key.clone(),
                        group,
                        in_shelf,
                        epoch: card_epoch.clone(),
                        order,
                        ring: tx.focus_visible(),
                    },
                );
                tx.live_only();
                return;
            }
            let suppressed = s.composer.focused || s.settings_open() || s.popup.is_open() || tx.focused() == Some("prompt");
            let ui = s.kernel.slots.get(&crate::restore::RESTORE).inbox;
            let order = s.inbox_order();
            let folds = Folds::from(ui);
            let (now_group, now_in_shelf) = crate::session::inbox::focus::card_location(&order, folds, ui.working_placement, &follow_key);
            let now_drafts = draft_ids(s);
            let now_items = items(&order, ui, now_drafts.len());
            let now_epoch = epoch(&now_items, &order);
            let now_order = crate::session::inbox::focus::visible_card_keys_from(&order, ui, &now_drafts);
            let effects = crate::session::inbox::focus::step(
                &mut s.sidebar_inbox.focus,
                crate::session::inbox::focus::FocusEvent::Blurred {
                    key: &follow_key,
                    now_group,
                    now_in_shelf,
                    now_epoch: &now_epoch,
                    now_order: &now_order,
                    suppressed,
                },
            );
            if effects.is_empty() {
                tx.live_only();
                return;
            }
            for effect in effects {
                let crate::session::inbox::focus::FocusEffect::Focus { key, ring, scroll } = effect;
                if scroll {
                    if let Some(entry_key) = key.strip_prefix("ib-") {
                        reveal(s, entry_key, tx);
                    }
                    s.sidebar_inbox.focus_next = Some((key.clone(), ring));
                    tx.wake_after(std::time::Duration::ZERO);
                }
                tx.focus(key, ring);
            }
        })
        .on_press_tx(move |s: &mut App, tx: &mut Tx| {
            if !tx.by_keys() { s.sidebar_inbox.focus.note_pointer(); }
            s.open_inbox_session(&id, tx);
        })
        .on_secondary_press(move |s: &mut App, press: rune::PointerPress, tx: &mut Tx| {
            s.sidebar_inbox.focus.note_pointer();
            open_menu(s, &menu_id, archived, press, tx);
        })
        .child(
            row()
                .key("head")
                .w_full()
                .h(16.0)
                .gap(8.0)
                .center_y()
                .child(el().key("avatar").size(16.0, 16.0).center().child(crate::icons::badge(s, p, project, &name)))
                // Title and project in their own direction (`inbox.tsx:610`, `:625`).
                .child(text(title).font(TITLE).color(p.text).grow(1.0).shrink().min_w(0.0).truncate().dir(rune::Direction::Auto))
                .children(pin),
        )
        .child(row().key("sub").w_full().h(16.0).pl(24.0).gap(8.0).center_y().children(server).child(place).child(end));
    // The routed session's tab becomes this card as the layout switches, Settings open over it or
    // not (`shell-active-{id}`): 28 tall in Side tabs, 54 here, on SETTLE; GLIDE from Top tabs.
    if matches!(&*s.route, Route::Session { id: r, server } if server == e.server && s.root_of(r) == info.id) {
        card = card.matched(format!("{}-{key}", super::tab_strip::ACTIVE)).matched_crop();
        if let Some(spring) = s.switch_spring() {
            card = card.motion(spring);
        }
    }
    if selected {
        card = card.fill(p.pressed);
    } else {
        card = card.hover(p.hover);
    }
    // An archived session is dimmed, but not while it waits on a person: it stands in Needs
    // attention then (`inbox.tsx` `InboxSessionCard`, opencode#66327).
    if archived && !selected && !e.waiting() {
        card = card.opacity(0.5);
    }
    card
}

/// A card's right-click menu: Pin (or Unpin), Rename, Export…, the read mark, Archive (or
/// Unarchive), then Delete…, each on the card's server.
fn open_menu(s: &mut App, key: &str, archived: bool, at: rune::PointerPress, tx: &mut Tx) {
    let (server, id) = s.inbox_target(key);
    let pin = PinRef::new(server.clone(), id.clone());
    let pinned = pins::state(s).doc.is_pinned(&pin);
    let target = crate::session_menu::Target::on(&server, &id, crate::session_menu::Spot::Row);
    let read = crate::servers::runtime::with_server(s, &server, |s| crate::session_menu::lines(s, &target).into_iter().find_map(|line| match line {
        crate::session_menu::Line::Item(i) if matches!(i.id, "read" | "unread") => Some(Item::new(i.id, i.title)),
        _ => None,
    })).flatten();
    // `inbox.tsx:830-845,989-998` (opencode#66576): Rename, Pin/Unpin, Mark read/unread, ---, Copy >, Export…, ---, Archive/Unarchive, Delete….
    let copy_sub = crate::servers::runtime::with_server(s, &server, |s| crate::session_menu::copy::submenu(s, &server, &id)).flatten();
    let mut menu = Menu::new(t!("command.category.session"))
        .item(Item::new("rename", t!("common.rename")))
        .item(Item::new("pin", if pinned { t!("sidebar.recent.unpin") } else { t!("sidebar.recent.pin") }))
        .items(read)
        .separator();
    if let Some(crate::session_menu::Line::Sub { id: sub_id, title, menu: sub }) = copy_sub {
        menu = menu.submenu(sub_id, title, sub);
    }
    let menu = menu
        .item(Item::new("export", format!("{}\u{2026}", t!("common.export"))).enabled(!s.exporting(&server, &id)))
        .separator()
        .item(if archived { Item::new("unarchive", t!("sidebar.inbox.unarchive")) } else { Item::new("archive", t!("common.archive")) })
        .item(Item::new("delete", format!("{}…", t!("common.delete"))));
    let key = key.to_owned();
    tx.instant();
    popup::open(s, menu, at, tx, move |s: &mut App, picked, tx| {
        tx.instant();
        match picked {
            // Pins are shared with the other sidebar layouts (`usePins`).
            Some("pin") => pins::update(s, |doc| if pinned { doc.unpin(&pin) } else { doc.pin(pin) }),
            Some("rename") => s.open_rename_dialog(&target),
            Some(action @ ("export" | "read" | "unread")) => crate::session_menu::run(s, &target, action, tx),
            Some(action) if action.starts_with("copy:") => crate::session_menu::run(s, &target, action, tx),
            Some("archive") => s.on_inbox_row(&key, |s, id| s.archive_session(id, tx)),
            Some("unarchive") => s.on_inbox_row(&key, |s, id| s.unarchive_session(id)),
            Some("delete") => {
                let (server, id) = s.inbox_target(&key);
                s.ask_delete_on(&server, &id);
            }
            _ => {}
        }
    });
}

/// The inbox lists sessions, not tabs, so drafts get their own group (`inbox.tsx` `drafts`,
/// opencode#65525): the open draft and every other draft tab still holding words, newest first.
/// Without it a draft left for a session can't be reached again from the inbox.
pub(crate) fn draft_ids(s: &App) -> Vec<String> {
    s.tabs
        .iter()
        .rev()
        .filter(|t| t.is_draft() && (matches!(&*s.route, Route::Draft(id) if *id == t.id) || s.draft_has_content(&t.id)))
        .map(|t| t.id.clone())
        .collect()
}

/// `InboxDraftCard`: the draft's first line (else "New session") with its project's avatar, and
/// the project under it; opens the draft, and its right-click menu discards it.
fn draft_card(s: &App, p: &Pal, tab: &crate::tabs::Tab) -> El {
    let selected = matches!(&*s.route, Route::Draft(id) if *id == tab.id);
    let project = (tab.server == crate::servers::runtime::current()).then(|| s.project_at(&tab.directory)).flatten();
    let name = project_or_dir(project, &tab.directory);
    let words = s.draft_text(&tab.id).unwrap_or_default();
    let title = words.trim().lines().next().filter(|l| !l.trim().is_empty()).map_or_else(|| t!("command.session.new").to_owned(), |l| l.trim().to_owned());
    let (open, menu) = (tab.uid(), tab.uid());
    let card = col()
        .key(format!("ib-{}", tab.id))
        .role("button")
        .label(title.clone())
        .w_full()
        .h(CARD_H)
        .padding(8.0)
        .gap(6.0)
        .radius(6.0)
        .focus_inset(p)
        .focusable(true)
        .focus_selected(selected)
        .on_press_tx(move |s: &mut App, tx: &mut Tx| s.select_tab(&open, tx))
        .on_secondary_press(move |s: &mut App, press: rune::PointerPress, tx: &mut Tx| {
            let uid = menu.clone();
            tx.instant();
            popup::open(s, Menu::new(t!("sidebar.inbox.section.drafts")).item(Item::new("discard", t!("sidebar.inbox.discardDraft"))), press, tx, move |s: &mut App, picked, tx| {
                if picked == Some("discard") {
                    tx.instant();
                    s.close_tab(&uid, tx);
                }
            });
        })
        .child(
            row()
                .key("head")
                .w_full()
                .h(16.0)
                .gap(8.0)
                .center_y()
                .child(el().key("avatar").size(16.0, 16.0).center().child(crate::icons::badge(s, p, project, &name)))
                .child(text(title).font(TITLE).color(p.text).grow(1.0).shrink().min_w(0.0).truncate().dir(rune::Direction::Auto)),
        )
        .child(row().key("sub").w_full().h(16.0).pl(24.0).center_y().child(text(name).font(LINE).color(p.muted).shrink().min_w(0.0).truncate().dir(rune::Direction::Auto)));
    if selected { card.fill(p.pressed) } else { card.hover(p.hover) }
}

/// Desktop's `pin` glyph (`packages/ui/src/icons/catalog.ts`).
static PIN: Icon = Icon {
    name: "pin",
    view: 16.0,
    body: r#"<path d="M9.5 2.5L13.5 6.5M10.5 3.5L7.5 6.5H5.5L9.5 10.5V8.5L12.5 5.5M7.5 8.5L2.5 13.5" stroke-linecap="round" stroke-linejoin="round" stroke="currentColor"/>"#,
};

/// Desktop's `server` glyph (`additional-icons.ts`), on a 20 pt grid drawn at 16.
static SERVER: Icon = Icon {
    name: "server",
    view: 20.0,
    body: r#"<rect x="3.35547" y="1.92969" width="13.2857" height="16.1429" stroke="currentColor"/><rect x="3.35547" y="11.9297" width="13.2857" height="6.14286" stroke="currentColor"/><rect x="12.8555" y="14.2852" width="1.42857" height="1.42857" fill="currentColor"/><rect x="10" y="14.2852" width="1.42857" height="1.42857" fill="currentColor"/>"#,
};
