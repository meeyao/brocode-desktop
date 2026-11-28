//! Manage models, the model list's sliders button (Desktop's `DialogManageModels`,
//! `providers/models/manage.tsx`, `manage.css`): every model the server lists, by provider, each
//! with a switch that shows or hides it in the list, and one per provider for all of its models.
//! The choices are the prefs' `user` visibility (`models.tsx:116-144`).

use std::collections::HashSet;

use rune::{Align, Color, El, Key, Side, TextEdit, TextStyle, Transition, Tx, col, el, icon, row, text};
use rune_theme_opencode::icons as ic;

use super::{Model, ModelAction, PickerHost, host, mark, search};
use crate::palette::{Elevate as _, Pal};
use crate::popover::{self as pop, MENU, Opt};
use crate::style::{UI, UI_MED};

/// The open dialog: its query, the providers folded shut, and the open provider options menu.
pub struct Manage {
    pub search: TextEdit,
    pub collapsed: HashSet<String>,
    pub menu: Option<String>,
}

/// `dialog-title` and `dialog-description` as `.search-dialog` sets them (`shell/search-list.css`,
/// `#64882`): 13 on 20 at 440 in `text-base`, the description muted.
const TITLE: TextStyle = TextStyle::new(13.0, 440.0).lh(20.0).tracking(-0.04);
const DESCRIPTION: TextStyle = TextStyle::new(13.0, 440.0).lh(20.0).tracking(-0.04);

impl super::Pickers {
    /// Opens Manage models with an empty search, its field taking the keys.
    pub fn open_manage(&mut self) {
        let mut search = TextEdit::new("");
        search.focused = true;
        self.manage = Some(Manage { search, collapsed: HashSet::new(), menu: None });
    }

    pub fn close_manage(&mut self) {
        self.manage = None;
    }

    /// Every model the query finds by provider, name or id (`manage.tsx:48-68`), each provider's
    /// together by name, the providers in Desktop's order.
    fn manage_groups(&self) -> Vec<(String, Vec<&Model>)> {
        let query = self.manage.as_ref().map_or("", |m| m.search.text.as_str()).trim().to_owned();
        // A model's speeds are not entries of their own (`settings/models/models.tsx` `listed`).
        let mut models: Vec<&Model> = self.models.iter()
            .filter(|m| m.speed.is_none())
            .filter(|m| query.is_empty() || search::score(&query, &[m.group.as_str(), m.name.as_str(), m.id.as_str()]).is_some())
            .collect();
        models.sort_by(|a, b| host::provider_rank(a).cmp(&host::provider_rank(b)).then_with(|| rune::text::compare_names(&a.name, &b.name)));
        let mut groups: Vec<(String, Vec<&Model>)> = Vec::new();
        for m in models {
            match groups.last_mut() {
                Some((p, list)) if *p == m.provider => list.push(m),
                _ => groups.push((m.provider.clone(), vec![m])),
            }
        }
        groups
    }

    fn set_shown(&mut self, keys: Vec<String>, shown: bool) {
        self.prefs.set_visibility(keys.iter().map(String::as_str), shown);
    }
}

/// The scrim and the dialog over the window while Manage models is open.
pub fn layer<S: PickerHost>(s: &S, p: &Pal) -> Vec<El> {
    let pk = s.pickers();
    let Some(m) = &pk.manage else { return Vec::new() };
    let groups = pk.manage_groups();
    let searching = !m.search.text.trim().is_empty();

    let field = crate::style::field(&m.search, UI)
        .key("manage-models-input")
        .single_line()
        .grow(1.0)
        .shrink()
        .min_w(0.0)
        .color(p.text)
        .placeholder(opencode_i18n::t!("common.search.placeholder"), p.faint)
        .caret(p.text, p.selection).selection_style(p.field_selection)
        .on_edit(|s: &mut S| s.pickers_mut().manage_search_mut())
        .on_input_key(|s: &mut S, ev, tx: &mut Tx| {
            if ev.key != Key::Escape || s.pickers().manage.as_ref().is_some_and(|m| m.search.is_composing()) {
                return false;
            }
            tx.instant();
            if let Some(m) = s.pickers_mut().manage.as_mut() && m.menu.take().is_some() {
                return true;
            }
            s.pickers_mut().close_manage();
            true
        });
    // `manage-models-search` (`providers/models/manage.css:1-9`, `#64882`): 8 pt sides & bottom,
    // 32 pt field, 6 pt gap to a full-width hairline across the dialog.
    let search_row = col().key("manage-models-search").w_full().px(8.0).pb(8.0).gap(6.0)
        .child(row().key("field").h(32.0).w_full().pl(8.0).pr(8.0).gap(8.0).center_y()
            .child(icon(&ic::MAGNIFYING_GLASS, 16.0).color(p.icon_muted))
            .child(field))
        .child(rune::hairline(p.border).key("manage-models-rule").w_full().mx(-8.0));

    let mut body = col().key("manage-models-list").w_full().px(8.0).pb(8.0).gap(8.0);
    if groups.is_empty() {
        let mut status = row().key("manage-models-empty").w_full().min_h(120.0).gap(4.0).center()
            .child(text(opencode_i18n::t!("dialog.model.empty")).font(UI).color(p.muted));
        if searching {
            status = status.child(text(format!("\"{}\"", m.search.text.trim())).font(UI).color(p.faint));
        }
        body = body.child(status);
    }
    let open_menu = m.menu.clone();
    let mut menu_keys: Option<(String, Vec<String>)> = None;
    for (provider, models) in &groups {
        let open = searching || !m.collapsed.contains(provider);
        let menu_here = open_menu.as_deref() == Some(provider.as_str());
        if menu_here {
            menu_keys = Some((provider.clone(), models.iter().map(|m| m.key.clone()).collect()));
        }
        // `provider-model-group` (`settings.css`): `bg-layer-01`, 8 pt radius, inset 0.5 pt `border-muted`.
        let mut section = col().key(format!("manage-provider-{provider}")).w_full().radius(8.0)
            .fill(p.layer1).box_shadow(Pal::ring_in(p.border_muted)).clip_soft(0.0);
        section = section.child(header::<S>(p, provider, models, open, searching, menu_here));
        if open {
            section = section.child(rows::<S>(s, p, models));
        }
        body = body.child(section);
    }

    let dialog = col()
        .elevate(p.overlay)
        .key("manage-models")
        .role("dialog")
        .label_static(opencode_i18n::t!("dialog.model.manage"))
        // `<Dialog size="large">` (`providers/models/manage.tsx`), clamped to the window by the
        // modal layer's padding (32 across, 96 down).
        .w_full()
        .max_w(crate::dialog::DialogSize::Large.width(f32::INFINITY))
        .h(crate::dialog::DialogSize::Large.height(f32::INFINITY))
        .shrink()
        .min_h(0.0)
        .radius(12.0)
        .fill(p.bg_base)
        .clip_soft(0.0)
        .transition(Transition::none())
        .motion(rune::Spring::NONE)
        .modal(true, |s: &mut S| s.pickers_mut().close_manage())
        // `.search-dialog`'s `dialog-header` (`#64882`): 14 pt above, 16 at the sides, 8 below the
        // title and its description, and Connect an integration at its end.
        .child(row().key("manage-models-header").w_full().pt(14.0).px(16.0).pb(8.0)
            .child(col().grow(1.0).shrink().gap(2.0)
                .child(text(opencode_i18n::t!("dialog.model.manage")).font(TITLE).color(p.text))
                .child(text(opencode_i18n::t!("dialog.model.manage.description")).font(DESCRIPTION).color(p.muted)))
            .children(s.picker_model_actions().contains(&ModelAction::Connect).then(|| connect::<S>(p))))
        .child(search_row)
        // `settings-panel`: no bar, as Desktop hides it (`manage.tsx:134-135`, `settings.css:593-607`).
        .child(crate::scroller::scroller("manage-models-scroll").hidden(col().w_full().grow(1.0).shrink().min_h(0.0), body));
    let scrim = el().key("manage-models-scrim").absolute(0.0, 0.0).w_full().h_full().fill(p.scrim).transition(Transition::none());
    let modal = col().key("manage-models-modal").absolute(0.0, 0.0).w_full().h_full().px(16.0).py(48.0).center().transition(Transition::none()).child(dialog).raise();
    let mut out = vec![scrim, modal];
    if let Some((provider, keys)) = menu_keys {
        let dismiss = |s: &mut S, tx: &mut Tx| {
            tx.instant();
            if let Some(m) = s.pickers_mut().manage.as_mut() {
                m.menu = None;
            }
        };
        out.push(
            el().key("manage-menu-modal")
                .role("none")
                .absolute(0.0, 0.0)
                .w_full()
                .h_full()
                .transition(Transition::none())
                .modal_tx(false, dismiss)
                .child(
                    pop::scrim::<S>("manage-menu-dismiss", dismiss)
                        .press_on_down()
                        .on_secondary_press(move |s: &mut S, _: rune::PointerPress, tx: &mut Tx| dismiss(s, tx)),
                )
                .child(provider_menu::<S>(s, p, &provider, keys))
                .raise(),
        );
    }
    out
}

/// A provider's header (`provider-group.tsx:222-270`, `manage.css:34-60`): icon, name, chevron to
/// the right of the name, and an `outline-dots` menu trigger pinned 16 pt from the end.
fn header<S: PickerHost>(p: &Pal, provider: &str, models: &[&Model], open: bool, searching: bool, menu_open: bool) -> El {
    let name = models.first().map_or(provider.to_owned(), |m| m.group.clone());
    let first = models.first().map_or(String::new(), |m| m.key.clone());
    let fold = provider.to_owned();
    let mut trigger = row().key("trigger").role("button").expanded(open).label(format!("{name} models")).w_full().min_h(36.0)
        .pl(12.0).pr(40.0).center_y().radius(6.0)
        .child(row().key("label").gap(8.0).center_y().shrink().min_w(0.0)
            .child(icon(mark::mark(&first), 16.0).color(p.icon))
            .child(text(name.clone()).font(UI_MED).color(p.text).truncate().shrink().min_w(0.0))
            .child(icon(&ic::CHEVRON_DOWN, 14.0).key("chevron").color(p.icon_muted).ml(-2.0)
                .rotate(if open { 0.0 } else { -std::f32::consts::FRAC_PI_2 }).mirror_in_rtl()));
    if !searching {
        trigger = trigger.hover(p.hover).press(p.pressed).on_press_tx(move |s: &mut S, tx: &mut Tx| {
            tx.instant();
            let Some(m) = s.pickers_mut().manage.as_mut() else { return };
            if !m.collapsed.remove(&fold) {
                m.collapsed.insert(fold.clone());
            }
        });
    }
    let menu_id = provider.to_owned();
    let more = el()
        .key(format!("manage-more-{provider}"))
        .role("button")
        .label_static(opencode_i18n::t!("common.moreOptions"))
        .expanded(menu_open)
        // `<IconButton size="small">` (`provider-group.tsx:269`, `icon-button.css:29-33`): 20 × 20, radius 4.
        .size(20.0, 20.0)
        .radius(4.0)
        .center()
        .fill(if menu_open { p.pressed } else { Color::TRANSPARENT })
        .hover(if menu_open { Color::TRANSPARENT } else { p.hover })
        .press(p.pressed)
        .pin(None, Some(12.0), Some(16.0), None)
        .press_on_down()
        .on_press_tx(move |s: &mut S, tx: &mut Tx| {
            tx.instant();
            let Some(m) = s.pickers_mut().manage.as_mut() else { return };
            m.menu = if m.menu.as_deref() == Some(menu_id.as_str()) { None } else { Some(menu_id.clone()) };
        })
        .child(icon(&ic::OUTLINE_DOTS, 16.0).color(p.icon_muted));
    row().key("header").w_full().padding(4.0).center_y().child(trigger).child(more)
}

/// The provider header's `⋯` menu (`provider-group.tsx:251-269`): Enable all / Disable all models.
fn provider_menu<S: PickerHost>(s: &S, p: &Pal, provider: &str, keys: Vec<String>) -> El {
    let enable_keys = keys.clone();
    let disable_keys = keys;
    let menu = pop::surface::<S>(p, MENU, format!("manage-menu-{provider}"), opencode_i18n::t!("common.moreOptions"))
        .w(180.0)
        .child(
            col().w_full().padding(MENU.inset).gap(2.0)
                .child(pop::option_tx(
                    p,
                    MENU,
                    Opt::new("enable-all", opencode_i18n::t!("settings.models.enableAll")),
                    |_: &mut S, _: &mut Tx| {},
                    move |s: &mut S, tx: &mut Tx| {
                        tx.instant();
                        s.pickers_mut().set_shown(enable_keys.clone(), true);
                        if let Some(m) = s.pickers_mut().manage.as_mut() {
                            m.menu = None;
                        }
                    },
                ))
                .child(pop::option_tx(
                    p,
                    MENU,
                    Opt::new("disable-all", opencode_i18n::t!("settings.models.disableAll")),
                    |_: &mut S, _: &mut Tx| {},
                    move |s: &mut S, tx: &mut Tx| {
                        tx.instant();
                        s.pickers_mut().set_shown(disable_keys.clone(), false);
                        if let Some(m) = s.pickers_mut().manage.as_mut() {
                            m.menu = None;
                        }
                    },
                )),
        );
    pop::place(s.knobs(), menu, &format!("manage-more-{provider}"), Side::Bottom, Align::End, 4.0)
}

/// A provider's models (`manage.tsx:74-99`, `manage.css:73-91`): 8 pt sides & bottom inset, a
/// 0.5 pt top rule, 4 pt gap, then 36 pt rows each flipping its switch.
fn rows<S: PickerHost>(s: &S, p: &Pal, models: &[&Model]) -> El {
    let pk = s.pickers();
    let mut list = col().key("rows").w_full().pt(4.0);
    for m in models {
        let shown = pk.visible(m);
        let key = m.key.clone();
        let press = key.clone();
        let r = row().key(format!("manage-{key}")).role("switch").label(m.name.clone()).w_full().h(36.0).px(8.0).gap(12.0).center_y().radius(6.0)
            .hover(p.hover).press(p.pressed)
            .on_press_tx(move |s: &mut S, tx: &mut Tx| {
                tx.instant();
                s.pickers_mut().set_shown(vec![press.clone()], !shown);
            })
            .child(text(m.name.clone()).font(UI_MED).color(p.text).grow(1.0).shrink().min_w(0.0).truncate())
            .child(switch::<S>(p, "switch", shown, move |s: &mut S| s.pickers_mut().set_shown(vec![key.clone()], !shown)));
        list = list.child(r);
    }
    col().key("models").w_full().px(8.0).pb(8.0)
        .child(rune::rule(0.5, p.border).key("rows-rule").w_full())
        .child(list)
}

/// Desktop's switch (`switch.css:20-152`): 24 × 16 with 4 pt corners, a 12 pt thumb with 2 pt
/// corners, track sheen, elevation, and depth-shaded thumb.
fn switch<S: PickerHost>(p: &Pal, key: &str, on: bool, flip: impl Fn(&mut S) + 'static) -> El {
    let label = if on { opencode_i18n::t!("project.settings.autoAccept.on") } else { opencode_i18n::t!("project.settings.autoAccept.off") };
    rune_kit::Switch::new(on)
        .web()
        .label(label)
        .hide_label()
        .on_toggle_tx(move |s: &mut S, tx: &mut Tx| {
            tx.instant();
            flip(s);
        })
        .build(crate::palette::kit(p))
        .key(key.to_owned())
}

/// The header's `button-v2` neutral at size small with its plus (`manage.tsx:107-109`): it
/// closes Manage models and hands Connect to the host.
#[allow(deprecated)]
fn connect<S: PickerHost>(p: &Pal) -> El {
    rune_kit::Button::new(ModelAction::Connect.label())
        .size(rune_kit::ButtonSize::Small)
        .icon(&ic::PLUS)
        .on_press_tx(|s: &mut S, tx: &mut Tx| {
            tx.instant();
            s.pickers_mut().close_manage();
            s.picker_model_action(ModelAction::Connect, tx);
        })
        .build(crate::palette::kit(p))
        .key("manage-models-connect")
        .mr(-2.0)
}
