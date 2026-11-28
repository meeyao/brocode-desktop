//! Plugin contributions drawn into the app's surfaces: panels and their tab labels, shell slots,
//! titlebar items, composer pills, popovers, dialogs, Settings rows and pages, file viewers and
//! code blocks. Every render goes through [`render`], memoized on what it read; a render that
//! fails draws its error card (a shell slot draws nothing), and the rest of the shell carries on.

use std::hash::{Hash, Hasher};

use mlua::Function;
use rune::{El, Tx, col, el, row, text};
use rune_luau::Data;
use session_view::palette::Elevate as _;

use crate::App;
use crate::palette::Pal;
use crate::plugin_host::plugin::{Contrib, Contribs};
use crate::plugin_host::services::{self, map};
use crate::plugin_host::{Flyout, FlyoutEvent, Life, OpenDialog, Overlay, commands, cx, dormant, epoch, icons, reload, requests};
use crate::view::UI_MED;

thread_local! {
    /// A render logged an error since [`render`] cleared it.
    static FAILED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Notes that the render running now failed (the scripts' log hook calls it on an error).
pub fn note_error() {
    FAILED.set(true);
}

/// Renders `f(view, props)` of plugin `i` as surface `surface`, entering the VM only when the
/// script ran, the props, the theme or what the render read last time changed. A shell slot
/// whose render fails draws nothing and isn't run again until the plugin reloads; the console
/// has the error. Other surfaces show the error card with Reload.
pub fn render(app: &App, p: &Pal, i: usize, surface: &str, f: &Function, props: Data) -> El {
    let plugin = &app.plugins.list[i];
    let Some(script) = plugin.script() else { return el() };
    let half = &plugin.half;
    let chrome = surface.starts_with("slot:");
    let settings_page = surface.starts_with("page:");
    if half.failed.borrow().contains(surface) {
        if chrome {
            return el();
        }
        if settings_page {
            return page_error_card(p, &plugin.manifest.id, &plugin.manifest.name, None);
        }
    }
    struct RestoreFailed(bool);
    impl Drop for RestoreFailed {
        fn drop(&mut self) {
            FAILED.set(self.0);
        }
    }
    let restore_failed = RestoreFailed(FAILED.replace(false));
    app.plugins.dark.set(p.dark);
    let reads = half.reads.borrow().get(surface).cloned().unwrap_or_default();
    let mut h = std::hash::DefaultHasher::new();
    (half.surface_version(Some(script)), epoch(), p.theme, p.generation, p.dark, cx::stamp(app, &reads)).hash(&mut h);
    format!("{props:?}").hash(&mut h);
    let version = h.finish();
    let before = script.metrics().renders;
    let props_for_bench = crate::flag("--luau-bench").then(|| props.clone());
    let trace_args = crate::files::trace::on().then(|| format!("{props:?}"));
    let t0 = crate::files::trace::on().then(rune::time::Instant::now);
    let id = half.id.clone();
    let (built, read) = cx::recording(|| cx::rendering_as(&id, || cx::enter(app, || script.render_fn::<App>(surface, version, p.dark, f, props))));
    let failed = FAILED.get();
    drop(restore_failed);
    if failed && (chrome || settings_page) {
        half.failed.borrow_mut().insert(surface.to_owned());
        if chrome {
            return el();
        }
        return page_error_card(p, &plugin.manifest.id, &plugin.manifest.name, None);
    }
    if script.metrics().renders > before {
        if let (Some(t0), Some(args_str)) = (t0, trace_args) {
            crate::files::trace::luau(&plugin.manifest.id, "render", surface, &args_str, t0.elapsed().as_micros() as u64);
        }
        bench(app, p, script, surface, f, &props_for_bench);
        half.reads.borrow_mut().insert(surface.to_owned(), read);
        app.plugins.renders.set(app.plugins.renders.get() + 1);
    } else {
        app.plugins.hits.set(app.plugins.hits.get() + 1);
    }
    // A render inside another plugin's (a file viewer under the Files panel's `ui.codeView`)
    // hands what it reads, and its own plugin's state, to the render around it, which otherwise
    // keeps the view it built until its own reads move.
    half.reads.borrow().get(surface).into_iter().flatten().for_each(|dep| cx::read(dep.clone()));
    cx::read(cx::Dep::Plugin(id));
    built
}

/// `--luau-bench`: the first three times a surface rebuilds (the first often runs before its
/// data arrives), runs its render 1,000 more times and prints the p50 and p99 time of one
/// rebuild (script and `El` construction): `rebuild`, every memo missing; `warm`, as a following
/// frame sees it, the surface missing and the script's own `view:Memo`s holding what didn't change.
fn bench(app: &App, p: &Pal, script: &rune_luau::Script, surface: &str, f: &Function, props: &Option<Data>) {
    thread_local! {
        static DONE: std::cell::RefCell<std::collections::HashMap<String, u32>> = std::cell::RefCell::default();
    }
    let Some(props) = props else { return };
    let key = surface.split(':').take(2).collect::<Vec<_>>().join(":");
    let nth = DONE.with_borrow_mut(|d| {
        let n = d.entry(key).or_default();
        *n += 1;
        *n
    });
    if nth > 3 {
        return;
    }
    let id = format!("bench:{surface}");
    for (name, seed, warm) in [("rebuild", 0x5eed_0000u64, false), ("warm", 0x3eed_0000, true)] {
        let mut times: Vec<f64> = (0..1000u64)
            .map(|n| {
                let start = rune::time::Instant::now();
                let run = || cx::enter(app, || script.render_fn::<App>(&id, n ^ seed, p.dark, f, props.clone()));
                let built = if warm { rune::memo::as_next_build(run) } else { run() };
                let us = start.elapsed().as_secs_f64() * 1e6;
                drop(built);
                us
            })
            .collect();
        times.sort_by(f64::total_cmp);
        eprintln!("luau bench: {} {surface}: {name}#{nth} p50 {:.1} us p99 {:.1} us max {:.1} us (n 1000)", script.name(), times[499], times[989], times[999]);
    }
}
/// Each running plugin's contributions of one kind, with the plugin's index.
fn each<'a, S: 'a, R>(app: &'a App, pick: impl Fn(&Contribs) -> Vec<R> + 'a) -> Vec<(usize, R)> {
    let _ = std::marker::PhantomData::<S>;
    app.plugins
        .listed()
        .flat_map(|(i, p)| pick(&p.half.contribs.borrow()).into_iter().map(move |r| (i, r)))
        .collect()
}

/// Asks each dormant plugin among `found` (from [`each`]) to start: `now` at the next turn, for
/// a surface that shows it, else in idle time. A dormant plugin's entries have no functions, so the
/// surface draws without them meanwhile.
fn want_asleep<R>(app: &App, found: &[(usize, R)], now: bool) {
    for (i, _) in found {
        let p = &app.plugins.list[*i];
        if !p.running() {
            if now { dormant::want_now(app, &p.manifest.id) } else { dormant::want(app, &p.manifest.id) }
        }
    }
}

/// What a render needs from a contribution: its key, a function and its spec's fields.
fn take<S: Clone>(list: &[Contrib<S>], f: &str) -> Vec<(&'static str, S, Option<Function>)> {
    list.iter().map(|c| (c.key, c.spec.clone(), c.f(f).cloned())).collect()
}

// ---- Slots, titlebar, composer -----------------------------------------------------------------

/// What plugins put in shell slot `name` (`window.bottom`, `session.header`) for session
/// `session`.
pub fn slot(app: &App, p: &Pal, name: &str, session: &str) -> Vec<El> {
    slot_with(app, p, name, session, true, false)
}

/// [`slot`] as the session header draws it: `active` for the routed session, `compact` in a
/// narrow header. In `order`.
pub fn slot_with(app: &App, p: &Pal, name: &str, session: &str, active: bool, compact: bool) -> Vec<El> {
    let directory = app.session(session).map_or("", |s| s.directory.as_str()).to_owned();
    let found = each::<(), _>(app, |c| c.slots.iter().filter(|s| s.spec.at.as_str() == name).map(|s| (s.spec.clone(), s.f("render").cloned())).collect());
    want_asleep(app, &found, true);
    let mut all = found
        .into_iter()
        .filter_map(|(i, (spec, f))| {
            let props = map([
                ("sessionId", Data::String(session.to_owned())),
                ("directory", Data::String(directory.clone())),
                ("active", Data::Bool(active)),
                ("compact", Data::Bool(compact)),
                ("shown", Data::Bool(true)),
            ]);
            Some((spec.order.unwrap_or(0), render(app, p, i, &format!("slot:{name}:{}:{session}", spec.id), &f?, props)))
        })
        .collect::<Vec<_>>();
    all.sort_by_key(|(order, _)| *order);
    all.into_iter().map(|(_, el)| el).collect()
}

/// What plugins put in a side-panel New tab slot (`session.panel.new`, `session.panel.new.list`).
pub fn new_tab_slot(
    app: &App,
    p: &Pal,
    name: &str,
    session: &str,
    pane: &str,
    width: f32,
    focus_token: Option<u64>,
) -> Vec<El> {
    let directory = app.session(session).map_or("", |s| s.directory.as_str()).to_owned();
    let found = each::<(), _>(app, |c| {
        c.slots.iter().filter(|s| s.spec.at.as_str() == name).map(|s| (s.spec.clone(), s.f("render").cloned())).collect()
    });
    want_asleep(app, &found, true);
    let mut all = found
        .into_iter()
        .filter_map(|(i, (spec, f))| {
            let mut props = map([
                ("sessionId", Data::String(session.to_owned())),
                ("directory", Data::String(directory.clone())),
                ("pane", Data::String(pane.to_owned())),
                ("width", Data::Number(f64::from(width))),
            ]);
            if let (Some(tok), Data::Map(m)) = (focus_token, &mut props) {
                m.insert("focusToken".into(), Data::Integer(tok as i64));
            }
            Some((spec.order.unwrap_or(0), render(app, p, i, &format!("slot:{name}:{}:{session}:{pane}", spec.id), &f?, props)))
        })
        .collect::<Vec<_>>();
    all.sort_by_key(|(order, _)| *order);
    all.into_iter().map(|(_, el)| el).collect()
}

/// Titlebar items: each plugin's `show()` answer as a [`titlebar_pill`](super::titlebar_pill),
/// in the titlebar or, `vertical`, the sidebar's footer.
fn titlebar_pills(app: &App, p: &Pal, vertical: bool) -> Vec<El> {
    let found = each::<(), _>(app, |c| c.titlebar_items.iter().map(|t| (t.key, t.f("show").cloned(), t.f("run").cloned())).collect());
    want_asleep(app, &found, false);
    found
        .into_iter()
        .filter_map(|(i, (key, show, run))| {
            let script = app.plugins.list[i].script()?;
            let view: opencode_plugin_spec::TitlebarItemView = cx::enter(app, || script.eval_fn::<Data>(show.as_ref()?, ()).ok())?.decode().ok()?;
            let plugin = app.plugins.list[i].manifest.id.as_str();
            let icon = view.icon.as_deref().map_or(&super::titlebar_pill::DOWNLOAD, |name| crate::plugin_host::icons::resolve(plugin, name));
            let pill = super::titlebar_pill::Pill { key, label: view.label, title: view.title, icon, busy: view.busy == Some(true) };
            let press = run.map(|run| move |s: &mut App, tx: &mut Tx| commands::spawn(s, i, &run, (), tx));
            Some(super::titlebar_pill::view(app, p, vertical, pill, press))
        })
        .collect()
}
/// Titlebar items at the trailing end of the horizontal titlebar row (`WindowTitlebar::trailing`),
/// including narrow windows regardless of tab layout (`shell.tsx:61`, `titlebar.tsx:1049-1053`).
/// In wide side tabs and the inbox, they belong in the sidebar's footer ([`sidebar_items`]).
pub fn titlebar_items(app: &App, p: &Pal) -> Option<El> {
    let chrome = crate::view::chrome();
    if chrome.row_h <= 0.0 || chrome.layout != crate::layout_state::TabLayout::Horizontal {
        return None;
    }
    let items = titlebar_pills(app, p, false);
    if items.is_empty() {
        return None;
    }
    Some(row().key("plugin-titlebar-items").gap(6.0).center_y().no_window_drag().children(items))
}

/// The sidebar's footer of titlebar items, in side tabs and the inbox (`vertical-tabs-footer`):
/// 8 under the list, a column of pills.
pub fn sidebar_items(app: &App, p: &Pal) -> Option<El> {
    let items = titlebar_pills(app, p, true);
    (!items.is_empty()).then(|| col().key("vertical-tabs-footer").mt(8.0).w_full().items_start().children(items))
}

/// The pills plugins put in the composer lane after the app's own (`SessionPluginPills`): their
/// `show()` answer in the lane's capsule; a pill with `renderMenu` opens it as a popover above,
/// one without runs `run`.
pub fn pill_views(app: &App, p: &Pal, session: &str) -> Vec<El> {
    let directory = app.session(session).map_or("", |s| s.directory.as_str()).to_owned();
    let found = each::<(), _>(app, |c| c.composer_pills.iter().map(|c| (c.key, c.spec.id.clone(), c.f("show").cloned(), c.f("renderMenu").is_some(), c.f("run").cloned())).collect());
    // A pill's plugin starts at the next turn: at a launch that is just after the first frame
    // (`first_screen`), so the pill comes as soon as its plugin can answer.
    want_asleep(app, &found, true);
    found
        .into_iter()
        .filter_map(|(i, (key, id, show, menu, run))| {
            let script = app.plugins.list[i].script()?;
            let props = map([("sessionId", Data::String(session.to_owned())), ("directory", Data::String(directory.clone()))]);
            let view: opencode_plugin_spec::ComposerPillView = cx::enter(app, || script.eval_fn::<Data>(show.as_ref()?, props.clone()).ok())?.decode().ok()?;
            let anchor = format!("plugin-pill-{key}");
            let plugin = app.plugins.list[i].half.id.clone();
            let open = app.plugins.overlay.pill_open(&plugin, &id);
            let name = view.title.clone().unwrap_or_else(|| view.label.clone());
            let ink = match view.tone {
                Some(opencode_plugin_spec::Tone::Accent) => p.v2.icon_accent,
                Some(t) => session_view::palette::token(p.dark, &format!("state-fg-{}", t.as_str())).unwrap_or(p.icon_muted),
                None => p.icon_muted,
            };
            let mut pill = crate::subagents::chip(app, p, &anchor, name, open).shrink().min_w(0.0);
            // A leading icon sits 8 pt from the edge and 6 pt from its label, in a 16 pt box
            // (`composer-lane.css:62-66`, `plugin-pills.tsx`'s `PillIcon`).
            if let Some(icon) = &view.icon {
                let glyph = rune::icon(crate::plugin_host::icons::resolve(&plugin, icon), 14.0).color(ink);
                pill = pill.pl(8.0).gap(6.0).child(el().size(16.0, 16.0).center().child(glyph));
            }
            pill = pill.child(text(view.label).font(crate::view::UI).color(p.text).truncate().max_w(260.0).shrink().min_w(0.0));
            Some(pill.on_press_tx(move |s: &mut App, tx: &mut Tx| {
                tx.instant();
                if menu {
                    if s.plugins.overlay.pill_open(&plugin, &id) {
                        return requests::close_popover(s, tx);
                    }
                    s.plugins.overlay = Overlay::PillMenu { plugin: plugin.clone(), id: id.clone(), anchor: anchor.clone(), data: props.clone() };
                    crate::popover::motion::opening(&s.diag.knobs, tx);
                } else if let Some(run) = &run {
                    commands::spawn(s, i, run, props.clone(), tx);
                }
            }))
        })
        .collect()
}

// ---- Popovers and dialogs ----------------------------------------------------------------------

/// The open plugin popover and the scrim under it that closes it.
pub fn popover_view(app: &App, p: &Pal) -> Vec<El> {
    let (plugin, id, data) = match &app.plugins.overlay {
        Overlay::Closed => return Vec::new(),
        Overlay::Popover { plugin, id, data, .. } | Overlay::PillMenu { plugin, id, data, .. } => (plugin, id, data),
    };
    let Some((i, _)) = app.plugins.running().find(|(_, pl)| pl.half.id == *plugin) else { return Vec::new() };
    let half = &app.plugins.list[i].half;
    let scrim = crate::popover::scrim::<App>("plugin-popover-scrim", requests::close_popover)
        .press_focuses_below()
        .on_secondary_press(|s: &mut App, _: rune::PointerPress, tx: &mut Tx| requests::close_popover(s, tx));
    let scrim = if crate::popup::PRESS_ON_DOWN { scrim.press_on_down() } else { scrim };
    if let Overlay::PillMenu { anchor, .. } = &app.plugins.overlay {
        let pill = half.contribs.borrow().composer_pills.iter().find(|c| c.spec.id == *id).map(|c| (c.key, c.f("renderMenu").cloned(), c.f("show").cloned()));
        let Some((key, Some(menu), show)) = pill else { return Vec::new() };
        let script = app.plugins.list[i].script();
        let view: Option<opencode_plugin_spec::ComposerPillView> = script.and_then(|s| cx::enter(app, || s.eval_fn::<Data>(show.as_ref()?, data.clone()).ok())).and_then(|d| d.decode().ok());
        let content = render(app, p, i, &format!("pill:{id}"), &menu, data.clone());
        // The host keeps only the frame: everything inside, any heading included, is the
        // plugin's, and the view's title (else its label) names it (`plugin-pills.tsx`).
        let name = view.map(|v| v.title.unwrap_or(v.label)).unwrap_or_default();
        let card = col()
            .key(format!("plugin-pill-popover-{key}"))
            .role("dialog")
            .label(name)
            .w(320.0)
            // `w-80 p-1` inside a 0.5 transparent border (`[data-composer-pill-panel]`).
            .padding(4.5)
            .radius(10.0)
            .fill(p.layer1)
            .elevate(p.floating)
            .on_press(|_: &mut App| {})
            .child(content);
        return wrap_popover_modal(app, vec![scrim, crate::popover::place(&app.diag.knobs, card, anchor, rune::Side::Top, rune::Align::Start, 8.0)]);
    }
    let Overlay::Popover { anchor, side, align, gap, flyout, .. } = &app.plugins.overlay else { return Vec::new() };
    let found = half.contribs.borrow().popovers.iter().find(|c| c.spec.id == *id).map(|c| (c.key, c.f("render").cloned()));
    let Some((key, Some(f))) = found else { return Vec::new() };
    let content = render(app, p, i, &format!("popover:{id}"), &f, map([("data", data.clone())]));
    let content = col().key(format!("plugin-popover-{key}")).on_press(|_: &mut App| {}).child(content);
    let mut out = vec![scrim, crate::popover::place(&app.diag.knobs, content, anchor, *side, *align, *gap)];
    out.extend(flyout_view(app, p, i, flyout));
    wrap_popover_modal(app, out)
}

/// Wraps a plugin popover's scrim and surface in a raised modal scope when another Rune modal sits
/// beneath `PLUGIN_TOP` and no `CHROME_POPUP` menu is open above the dialog, so outside clicks and
/// the popover surface are not blocked by the modal underneath.
fn wrap_popover_modal(app: &App, out: Vec<El>) -> Vec<El> {
    if !crate::popup::modal_open_below_popup(app) || app.popup.is_open() {
        return out;
    }
    vec![
        el().key("plugin-popover-modal")
            .role("none")
            .absolute(0.0, 0.0)
            .w_full()
            .h_full()
            .transition(rune::Transition::none())
            .modal_tx(false, requests::close_popover)
            .children(out)
            .raise(),
    ]
}

/// The list a popover row opened, hung beside its row (drawn after the popover that holds the
/// row): its top 2 above the row's, so its first row lines up with it across the list's 2 inset
/// (`session-service-flyout`, `translateY(row - 2)`), 2 out from the row, which sits 2 inside
/// the card, so it meets the card's edge.
fn flyout_view(app: &App, p: &Pal, i: usize, flyout: &Flyout) -> Option<El> {
    let list = flyout.list()?;
    let half = &app.plugins.list[i].half;
    let (key, f) = half.contribs.borrow().popovers.iter().find(|c| c.spec.id == list.id).map(|c| (c.key, c.f("render").cloned()))?;
    let content = render(app, p, i, &format!("flyout:{}", list.id), &f?, map([("data", list.data.clone())]));
    // An anchored element keeps its anchor's top whatever its margin, so the list moves up inside it.
    let body = col()
        .mt(-2.0)
        // Where the list is, for the rows' safe triangle; drawing doesn't depend on it.
        .on_layout_tx(|s: &mut App, rect, tx: &mut Tx| {
            s.plugins.aim.list.set(Some(rect));
            tx.live_only();
        })
        // The pointer reached the list: the aim ends (`aim.cancelled`).
        .on_hover_change(|s: &mut App, inside: bool, tx: &mut Tx| {
            let aiming = matches!(&s.plugins.overlay, Overlay::Popover { flyout, .. } if matches!(**flyout, Flyout::Aiming { .. }));
            if !(inside && aiming) {
                return tx.live_only();
            }
            requests::step_flyout(s, None, FlyoutEvent::Settled, tx);
        })
        .child(content);
    Some(col().key(format!("plugin-flyout-{key}")).anchored(list.row.clone(), list.side, rune::Align::Start, 2.0).on_press(|_: &mut App| {}).child(body))
}

/// The open plugin dialog's card sized for `win`, without its modal scrim or focus trap (for
/// standalone story stages as well as [`dialog_view`]).
pub fn dialog_card(app: &App, p: &Pal, win: [f32; 2]) -> Option<(&'static str, El)> {
    dialog_card_at(app, p, win).map(|(key, card, _)| (key, card))
}

/// [`dialog_card`], and the top edge the card keeps when it's one that only grows downward (a
/// tall one: `dialog.css:163-170`).
fn dialog_card_at(app: &App, p: &Pal, win: [f32; 2]) -> Option<(&'static str, El, Option<f32>)> {
    use crate::widgets::{DialogSize, Fit};
    let open = app.plugins.dialog.as_ref()?;
    let (i, _) = app.plugins.running().find(|(_, pl)| pl.half.id == open.plugin)?;
    let found = app.plugins.list[i].half.contribs.borrow().dialogs.iter().find(|c| c.spec.id == open.id).map(|c| (c.key, c.spec.clone(), c.f("render").cloned()));
    let (key, spec, Some(f)) = found? else { return None };
    let size = match spec.size {
        Some(opencode_plugin_spec::DialogSize::Tall) => DialogSize::Tall,
        Some(opencode_plugin_spec::DialogSize::Large) => DialogSize::Large,
        Some(opencode_plugin_spec::DialogSize::XLarge) => DialogSize::XLarge,
        _ => DialogSize::Normal,
    };
    // A tall one is `<Dialog size="tall" fit>` (`site-chooser.tsx:356`): as tall as its content up
    // to its size's height, on bg-base like the larger ones (`dialog.css:172-176`).
    let (fit, default_radius, fill) = match size {
        DialogSize::Normal => (Fit::Content, 6.0, p.layer1),
        DialogSize::Tall => (Fit::Content, 12.0, p.bg_base),
        DialogSize::Large | DialogSize::XLarge => (Fit::Fixed, 12.0, p.bg_base),
    };
    let radius = spec.radius.map_or(default_radius, |r| r as f32);
    let content = render(app, p, i, &format!("dialog:{}", open.id), &f, map([("data", open.data.clone())]));
    // The clip goes on a child: on the card it would clip the card's own shadow too.
    // A normal one's content scrolls past its height, as `DialogBody`'s `overflow-y-auto` box does
    // in `runtime/plugin/ui.tsx`; a large one's fills it.
    let inner = col().w_full().clip_rounded(radius);
    let inner = if fit == Fit::Fixed {
        inner.h_full().child(content)
    } else if size == DialogSize::Tall {
        // Its body doesn't scroll as a whole: at the size's height, what in it may shrink (its
        // list) gives way and scrolls by itself (`site-chooser.tsx:375-378`, `min-h-0`).
        inner.shrink().min_h(0.0).child(content)
    } else if spec.hide_scrollbar == Some(true) {
        inner.shrink().min_h(0.0).child(session_view::scroller::scroller(format!("plugin-dialog-scroll-{key}")).hidden(col().w_full().shrink().min_h(0.0), content))
    } else {
        inner.shrink().min_h(0.0).child(session_view::scroller::scroller(format!("plugin-dialog-scroll-{key}")).build::<App>(p, col().w_full().shrink().min_h(0.0), content))
    };
    let card = col()
        .key(format!("plugin-dialog-{key}"))
        .role("dialog")
        .label(spec.title.clone().unwrap_or_default())
        .radius(radius)
        .fill(fill)
        .elevate(p.overlay)
        .transition(rune::Transition::none())
        .child(inner);
    let card = crate::widgets::sized(card, size, fit, win);
    Some((key, card, (size == DialogSize::Tall).then(|| size.top(win[1]))))
}

/// Which modal overlay layer the currently open plugin dialog belongs to (defaulting to
/// `DialogLayer::Elevated` when omitted on `DialogSpec`).
pub fn dialog_layer(app: &App) -> Option<opencode_plugin_spec::DialogLayer> {
    let open = app.plugins.dialog.as_ref()?;
    let (i, _) = app.plugins.running().find(|(_, pl)| pl.half.id == open.plugin)?;
    let layer = app.plugins.list[i]
        .half
        .contribs
        .borrow()
        .dialogs
        .iter()
        .find(|c| c.spec.id == open.id)?
        .spec
        .layer
        .unwrap_or(opencode_plugin_spec::DialogLayer::Elevated);
    Some(layer)
}

/// The open plugin dialog over a scrim, as the app's own dialogs sit: in Desktop's dialog frame
/// at the plugin's size preset (`runtime/plugin/ui.tsx` `Dialog`): a normal one as tall as its
/// content (`<Dialog fit>`), a tall one as tall as its content up to its size's height with its top
/// edge kept, a large or x-large one its size's height, each of those on bg-base with 12 pt corners.
/// Esc or a press outside closes it and runs its `onDismiss`.
pub fn dialog_view(app: &App, p: &Pal, cx: &rune::Cx<App>) -> Vec<El> {
    let Some((key, card, top)) = dialog_card_at(app, p, crate::widgets::dialog_window(cx, *app.zoom)) else { return Vec::new() };
    let card = card.modal_tx(true, requests::dismiss_dialog);
    crate::widgets::modal(&format!("plugin-dialog-modal-{key}"), p, card, top)
}

/// Renders the open plugin dialog only when its declared [`opencode_plugin_spec::DialogLayer`]
/// matches `target`.
pub fn dialog_view_in(app: &App, p: &Pal, cx: &rune::Cx<App>, target: opencode_plugin_spec::DialogLayer) -> Vec<El> {
    if dialog_layer(app) != Some(target) {
        return Vec::new();
    }
    dialog_view(app, p, cx)
}

/// A server source's entry `rec` (key `key`), as its window half's `row` draws it in
/// Settings › Servers, with its latest health check.
pub fn server_row(app: &App, p: &Pal, key: &str, rec: &crate::plugin_host::ServerRec) -> Option<El> {
    let plugin = key.split_once(':')?.0;
    let (i, f) = app.plugins.running().find_map(|(i, pl)| {
        (pl.manifest.id == plugin).then(|| pl.half.contribs.borrow().server_sources.iter().find(|c| c.spec.id == rec.source).and_then(|c| c.f("row").cloned())).flatten().map(|f| (i, f))
    })?;
    let health = health_data(crate::servers::health::get(&opencode_kit::servers::ServerKey::new(key)).as_ref());
    let props = map([("id", Data::String(rec.id.clone())), ("key", Data::String(key.to_owned())), ("health", health)]);
    Some(render(app, p, i, &format!("server:{key}"), &f, props))
}

/// A server's health as plugins read it (`ServerHealth`, `toPluginHealth`): it has no stages,
/// so a server that isn't ready, or not checked yet, has none.
pub fn health_data(health: Option<&opencode_servers::health::Health>) -> Data {
    use opencode_servers::health::{Health, ProbeResult};
    match health {
        Some(Health::Probe(ProbeResult::Healthy { version, .. })) => map([("healthy", Data::Bool(true)), ("version", version.clone().map_or(Data::Nil, Data::String))]),
        Some(Health::Checking { managed: true }) => map([("healthy", Data::Bool(false)), ("checking", Data::Bool(true))]),
        Some(Health::Probe(ProbeResult::Unreachable | ProbeResult::Unauthorized)) => map([("healthy", Data::Bool(false))]),
        Some(Health::Incompatible) => map([("healthy", Data::Bool(false)), ("incompatible", Data::Bool(true))]),
        _ => Data::Nil,
    }
}

/// A server source's cover over tab `tab` of its entry `rec` (key `key`), which isn't ready
/// (`PluginServerCover`): its window half's `cover`, filling the tab's area. None when the
/// source draws none.
pub fn server_cover(app: &App, p: &Pal, key: &str, rec: &crate::plugin_host::ServerRec, tab: &str) -> Option<El> {
    let plugin = key.split_once(':')?.0;
    let (i, f) = app.plugins.running().find_map(|(i, pl)| {
        (pl.manifest.id == plugin).then(|| pl.half.contribs.borrow().server_sources.iter().find(|c| c.spec.id == rec.source).and_then(|c| c.f("cover").cloned())).flatten().map(|f| (i, f))
    })?;
    let props = map([
        ("id", Data::String(rec.id.clone())),
        ("key", Data::String(key.to_owned())),
        ("tab", Data::String(tab.to_owned())),
        ("visit", Data::Integer(app.plugins.cover_visit.visit())),
        ("dialog", Data::Bool(crate::plugin_host::servers::dialog_shows(app))),
    ]);
    let body = render(app, p, i, &format!("cover:{key}"), &f, props);
    // Sized by the edges `view` pins it to (the main area under the titlebar), not the window's.
    Some(col().key(format!("plugin-server-cover-{key}")).fill(p.bg_base).on_press(|_: &mut App| {}).child(body))
}

// ---- Settings ----------------------------------------------------------------------------------

/// Settings › Plugins rows a plugin added (`AddSetting`), as `(key, title, description, control)`.
pub fn setting_rows(app: &App, p: &Pal, i: usize) -> Vec<(String, String, String, El)> {
    let plugin = &app.plugins.list[i];
    let rows: Vec<_> = plugin.half.contribs.borrow().settings.iter().map(|c| (c.spec.clone(), c.f("render").cloned())).collect();
    rows.into_iter()
        .map(|(spec, f)| {
            let control = f.map_or_else(el, |f| render(app, p, i, &format!("setting:{}", spec.id), &f, Data::Nil));
            (format!("plugin-{}-{}", plugin.manifest.id, spec.id), spec.title, spec.description.unwrap_or_default(), control)
        })
        .collect()
}

/// The settings pages plugins add: on host page `page` as sections, or as pages of their own
/// (`page` None), by key.
fn page_specs(app: &App, page: Option<&str>) -> Vec<(usize, &'static str, opencode_plugin_spec::SettingsPageSpec, Option<Function>)> {
    each::<(), _>(app, |c| c.settings_pages.iter().map(|s| (s.key, s.spec.clone(), s.f("render").cloned())).collect())
        .into_iter()
        .filter(|(_, (_, spec, _))| spec.page.map(|p| p.as_str()) == page)
        .map(|(i, (key, spec, f))| (i, key, spec, f))
        .collect()
}

/// What page `spec` of plugin `i` draws at the end of its header, when it draws anything.
fn page_actions(app: &App, p: &Pal, i: usize, spec: &opencode_plugin_spec::SettingsPageSpec) -> Option<El> {
    let f = app.plugins.list[i].half.contribs.borrow().settings_pages.iter().find(|c| c.spec.id == spec.id).and_then(|c| c.f("renderActions").cloned())?;
    Some(render(app, p, i, &format!("page-actions:{}", spec.id), &f, Data::Nil))
}

fn page_body(app: &App, p: &Pal, i: usize, spec: &opencode_plugin_spec::SettingsPageSpec, f: &Function) -> El {
    let target = crate::settings::search::revealed_target(app).map(str::to_owned);
    render(app, p, i, &format!("page:{}", spec.id), f, map([("target", target.map_or(Data::Nil, Data::String))]))
}

/// Whether plugin `plugin_id` is currently running and contributing a live `SettingsPage` on host
/// page `page` (such as `"updater"` on `"general"`).
pub fn has_live_settings_section(app: &App, plugin_id: &str, page: &str) -> bool {
    app.plugins.running().any(|(_, p)| {
        p.manifest.id == plugin_id
            && p.half.contribs.borrow().settings_pages.iter().any(|s| {
                s.spec.page.map(|hp| hp.as_str()) == Some(page) && s.f("render").is_some()
            })
    })
}

/// The settings pages plugins add on host page `page` (`general`, `servers`), as sections of
/// that page (`PluginSettingsSections`). A section draws its own title and list
/// (`ui.settingsList`), as Desktop's do.
pub fn settings_pages(app: &App, p: &Pal, page: &str) -> Vec<El> {
    let found = page_specs(app, Some(page));
    let indices: Vec<(usize, ())> = found.iter().map(|(i, ..)| (*i, ())).collect();
    want_asleep(app, &indices, true);
    found
        .into_iter()
        .filter_map(|(i, key, spec, f)| Some(col().key(format!("plugin-page-{key}")).w_full().child(page_body(app, p, i, &spec, f.as_ref()?))))
        .collect()
}

/// The plugins' own Settings pages, in order, by key, each with whether it belongs beside
/// Experimental and About (`group: "about"`).
pub fn settings_page_ids(app: &App) -> Vec<(String, bool)> {
    page_specs(app, None).into_iter().map(|(_, key, spec, _)| (key.to_owned(), spec.about == Some(true))).collect()
}

fn own_page(app: &App, key: &str) -> Option<(usize, &'static str, opencode_plugin_spec::SettingsPageSpec, Option<Function>)> {
    page_specs(app, None).into_iter().find(|(_, k, spec, _)| *k == key || spec.id == key)
}

/// The key of the plugin page a manifest's search entry names by its id.
pub fn settings_page_key(app: &App, id: &str) -> String {
    own_page(app, id).map_or_else(|| id.to_owned(), |(_, key, _, _)| key.to_owned())
}

pub fn settings_page_title(app: &App, key: &str) -> Option<String> {
    own_page(app, key).map(|(_, _, spec, _)| spec.title)
}

/// The controls a plugin's page draws at the end of its header (`renderActions`).
pub fn settings_page_actions(app: &App, p: &Pal, key: &str) -> Option<El> {
    let (i, _, spec, _) = own_page(app, key)?;
    page_actions(app, p, i, &spec)
}

/// A plugin page's rail icon: one of the plugin's own (`icons/<name>.svg`) or a host icon by name.
pub fn settings_page_icon(app: &App, key: &str) -> Option<&'static rune::Icon> {
    own_page(app, key).and_then(|(i, _, spec, _)| Some(icons::resolve(&app.plugins.list[i].manifest.id, &spec.icon?)))
}

/// Error card with Reload shown when a plugin Settings page fails to render or its plugin errored.
fn page_error_card(p: &Pal, id: &str, name: &str, why: Option<&str>) -> El {
    let plugin_id = id.to_owned();
    let title = format!("{name} settings failed to render");
    let desc = why
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .unwrap_or_else(|| "This settings page stopped after a script error. Reload the plugin to try again.".to_owned());
    let btn = row()
        .key(format!("plugin-page-reload-{id}"))
        .role("button")
        .label("Reload")
        .h(28.0)
        .px(10.0)
        .center()
        .radius(6.0)
        .border(0.5, p.border)
        .hover(p.hover)
        .press(p.pressed)
        .on_press_tx(move |app: &mut App, tx: &mut Tx| {
            tx.instant();
            if dormant::wake(app, &plugin_id, dormant::Why::Use) {
                return;
            }
            if let Some(idx) = app.plugins.find(&plugin_id) {
                app.plugins.list[idx].half.failed.borrow_mut().clear();
                reload(app, idx, None);
            }
        })
        .child(text("Reload").font(UI_MED).color(p.text));
    crate::settings::settings_card(
        p,
        &format!("plugin-page-error-{id}"),
        std::iter::once((format!("plugin-page-error-row-{id}"), title.as_str(), desc, btn)),
    )
}

/// A plugin page: its title, description and what it draws.
pub fn settings_page(app: &App, p: &Pal, key: &str) -> Option<(String, String, El)> {
    let (i, _, spec, f) = own_page(app, key)?;
    let body = match f.as_ref() {
        Some(f) => page_body(app, p, i, &spec, f),
        None => {
            let plugin = &app.plugins.list[i];
            if matches!(plugin.life, Life::Errored { .. }) {
                page_error_card(p, &plugin.manifest.id, &plugin.manifest.name, plugin.life.why())
            } else {
                want_asleep(app, &[(i, ())], true);
                return None;
            }
        }
    };
    Some((spec.title.clone(), spec.description.clone().unwrap_or_default(), body))
}

// ---- Files and code blocks ---------------------------------------------------------------------

thread_local! {
    /// The host's own view of the code block a plugin renders now, for `ui.genUi` to place.
    static NATIVE: std::cell::RefCell<Option<El>> = const { std::cell::RefCell::new(None) };
}

/// Takes the host's view of the block being rendered (`ui.genUi`); None outside a code block.
pub fn take_native() -> Option<El> {
    NATIVE.take()
}

/// The running plugin and code block that render ```` ```language ```` blocks.
fn code_block_of(app: &App, language: &str) -> Option<(usize, opencode_plugin_spec::CodeBlockSpec, Option<Function>)> {
    let found: Vec<_> = each::<(), _>(app, |c| c.code_blocks.iter().map(|c| (c.spec.clone(), c.f("render").cloned())).collect())
        .into_iter()
        .filter(|(_, (spec, _))| spec.languages.iter().any(|l| l.eq_ignore_ascii_case(language)))
        .collect();
    want_asleep(app, &found, true);
    found.into_iter().find(|(i, _)| app.plugins.list[*i].running()).map(|(i, (spec, f))| (i, spec, f))
}

pub fn code_block_lang(app: &App, language: &str) -> bool {
    code_block_of(app, language).is_some()
}

/// A plugin's view of one ```` ```language ```` block (`CodeBlock.render`), `native` the host's
/// own view of it, which `ui.genUi` places; `version` moves whenever `native` does.
#[allow(clippy::too_many_arguments)]
pub fn code_block(app: &App, p: &Pal, language: &str, key: &str, code: &str, streaming: bool, version: u64, native: El) -> Option<El> {
    let Some((i, spec, Some(f))) = code_block_of(app, language) else { return Some(native) };
    let props = map([
        ("code", Data::String(code.to_owned())),
        ("language", Data::String(language.to_ascii_lowercase())),
        ("sessionId", app.active_session_id().map_or(Data::Nil, |s| Data::String(s.to_owned()))),
        ("streaming", Data::Bool(streaming)),
        // Not the contract's: what the host's own view draws from, so a memo of it moves too.
        ("revision", Data::Integer(version as i64)),
    ]);
    struct RestoreNative(Option<El>);
    impl Drop for RestoreNative {
        fn drop(&mut self) {
            NATIVE.set(self.0.take());
        }
    }
    let _restore_native = RestoreNative(NATIVE.replace(Some(native)));
    let built = render(app, p, i, &format!("code:{}:{key}", spec.id), &f, props);
    Some(built)
}

/// A plugin's view of file `path`, when a file viewer takes its extension.
pub fn file_viewer(app: &App, p: &Pal, session: &str, directory: &str, path: &str) -> Option<El> {
    let ext = std::path::Path::new(path).extension()?.to_string_lossy().to_ascii_lowercase();
    let found: Vec<_> = each::<(), _>(app, |c| take(&c.file_viewers, "render")).into_iter().filter(|(_, (_, s, _))| s.extensions.iter().any(|e| e.trim_start_matches('.').eq_ignore_ascii_case(&ext))).collect();
    want_asleep(app, &found, true);
    let (i, (key, spec, f)) = found.into_iter().find(|(i, _)| app.plugins.list[*i].running())?;
    let abs = if std::path::Path::new(path).is_absolute() { path.to_owned() } else { format!("{}/{path}", directory.trim_end_matches(['/', '\\'])) };
    let props = map([("path", Data::String(abs.clone())), ("sessionId", Data::String(session.to_owned())), ("directory", Data::String(directory.to_owned()))]);
    let body = render(app, p, i, &format!("file:{}:{abs}", spec.id), &f?, props);
    // `artifact-view.tsx:168`: a `flex min-h-0 flex-1 flex-col` container so the viewer owns its
    // scrollport, pinned zoom controls and centred placeholders (`office-scroll`, `office-workbook`).
    Some(col().key(format!("plugin-file-{key}")).w_full().grow(1.0).min_h(0.0).child(body))
}

// `--scene plugin-dialog`: the updater's beta notes dialog with 30 notes, as its `Dialog:Open`
// leaves it; past the normal height its content scrolls.
rune_register::register!(crate::scenes::SCENE_LIST: crate::scenes::Scene = [
    ("plugin-dialog", |s| {
        let note = |i: usize| serde_json::json!({ "sha": format!("{i:07}"), "type": "feat", "summary": format!("Change number {i} to the app, described in a sentence long enough to wrap") });
        let notes = serde_json::json!({ "notes": { "added": (1..=20).map(note).collect::<Vec<_>>(), "fixed": (21..=30).map(note).collect::<Vec<_>>(), "total": 30, "more": false }, "previous": "1.0.0" });
        s.plugins.dialog = Some(OpenDialog { plugin: "updater".into(), id: "beta".into(), data: services::json_to_data(&notes) });
    }),
]);
