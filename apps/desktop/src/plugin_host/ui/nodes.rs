//! Desktop-specific contract `ui.*` host nodes (`shimmer`, `switch`, `field`, `pill`,
//! `settingsRow`, `settingsList`, `flyoutRow`, `dialogHeader`, `markdown`, `genUi`, `image`,
//! `video`). `App`-independent UI primitives live in `opencode_plugin_runtime::ui_nodes`.

use opencode_plugin_runtime::ui_nodes::{self, dir, named_icon, text_style};
use opencode_ui::popup::MenuButton as _;
use rune::{El, TextStyle, Tx, el, row, text};
use rune_luau::{Data, HostNode, NodeArgs};

use crate::App;
use crate::palette::{Pal, kit, pal};
use crate::plugin_host::{FlyoutEvent, bump, cx, flyout, input_changed, key_pressed, native, requests, services, submitted};

pub fn all() -> Vec<HostNode> {
    let mut all = ui_nodes::base_nodes::<App>(
        |id| cx::with_app(|app| crate::keymap::command_chord(app, id)).flatten(),
        |command| cx::with_app(|app| app.catalog().into_iter().find(|c| c.id == command).and_then(|c| c.chord)).flatten(),
    );
    all.extend([
        ui_nodes::node("image", services::images::node),
        ui_nodes::node("video", services::images::video_node),
        ui_nodes::node("shimmer", shimmer),
        ui_nodes::node("switch", switch),
        ui_nodes::node("select", select),
        ui_nodes::node("field", field),
        ui_nodes::node("pill", pill),
        ui_nodes::node("settingsRow", settings_row),
        ui_nodes::node("settingsList", settings_list),
        ui_nodes::node("flyoutRow", flyout_row),
        ui_nodes::node("dialogHeader", dialog_header),
        ui_nodes::node("markdown", markdown),
        ui_nodes::node("genUi", gen_ui),
    ]);
    all.extend(native::nodes());
    #[cfg(feature = "office")]
    all.extend(super::office_nodes::all());
    all
}

/// `ui.genUi`: a ```` ```ui ```` outline as parts (`rune_parts`). In a reply's code block it is
/// the timeline's own view of that block, with Find's marks, selection and the reply's width;
/// anywhere else it parses `source` itself at the content width.
fn gen_ui(a: &mut NodeArgs) -> Result<El, String> {
    if let Some(native) = super::surfaces::take_native() {
        return Ok(native);
    }
    let width = a.number("w").map_or_else(session_view::style::content_w, |w| w as f32);
    Ok(session_view::genui::standalone::<App>(&p(a), a.string("source").unwrap_or_default(), width, a.flag("streaming")).unwrap_or_else(el))
}

fn p(a: &NodeArgs) -> Pal {
    *pal(a.dark())
}

fn shimmer(a: &mut NodeArgs) -> Result<El, String> {
    let words = a.content().unwrap_or_default().to_owned();
    let font = text_style(a, TextStyle::new(13.0, 440.0));
    let pal = p(a);
    cx::with_app(|app| session_view::working::shimmer(app, &pal, &words, font, *app.clock)).ok_or_else(|| "ui.shimmer draws only in a render".into())
}

/// Desktop's `Select` dropdown control (`packages/ui/src/forms/select/select.css:22-45`,
/// contract 1.12): renders `rune_kit::Select` and opens a `Menu::picker` anchored to its trigger.
fn select(a: &mut NodeArgs) -> Result<El, String> {
    let pal = p(a);
    let key = a.string("key").unwrap_or("select").to_owned();
    let label = a.string("label").unwrap_or_default().to_owned();
    let current = a.string("value").unwrap_or_default().to_owned();
    let placeholder = a.string("placeholder").unwrap_or_default().to_owned();
    let disabled = a.flag("disabled");
    let options: Vec<(String, String)> = match a.data("options") {
        Some(Data::List(items)) => items
            .iter()
            .filter_map(|item| {
                let value = item.get("value").as_str()?.to_owned();
                let title = item.get("label").as_str().unwrap_or(&value).to_owned();
                Some((value, title))
            })
            .collect(),
        _ => Vec::new(),
    };
    let shown = options
        .iter()
        .find(|(v, _)| *v == current)
        .map(|(_, l)| l.clone())
        .unwrap_or_else(|| placeholder.clone());
    let anchor = format!("{key}-trigger");
    let open = cx::with_app(|app| app.popup.open_at(&anchor)).unwrap_or(false);
    let mut trigger = rune_kit::Select::new(key, &[shown.as_str()])
        .current((!shown.is_empty()).then_some(shown.as_str()))
        .label(label.clone())
        .open(open)
        .trigger(kit(&pal));
    if disabled {
        return Ok(opencode_ui::widgets::not_allowed(trigger.disabled(true).layer_opacity(0.5), true));
    }
    let on_change = a.press_later("onChange");
    trigger = trigger.menu_button(anchor.clone(), move |s: &mut App, tx: &mut Tx| {
        let menu = crate::popup::Menu::picker(label.clone(), options.clone(), &current)
            .align(rune::Align::Start)
            .min_width(160.0)
            .gap(6.0);
        let on_change = on_change.clone();
        crate::popup::open(s, menu, anchor.clone(), tx, move |s: &mut App, picked, tx| {
            if let Some(id) = picked {
                tx.instant();
                if let Some(cb) = &on_change {
                    cb.run(s, tx, vec![("value", Data::String(id.to_owned()))]);
                }
            }
        });
    });
    Ok(trigger)
}

fn switch(a: &mut NodeArgs) -> Result<El, String> {
    let checked = a.flag("checked");
    let key = a.string("key").unwrap_or("switch").to_owned();
    let s = crate::settings::switch(&p(a), &key, checked);
    if a.flag("disabled") {
        return Ok(s.disabled(true).layer_opacity(0.5).hit().cursor(rune::Cursor::NotAllowed));
    }
    Ok(a.on_press_with(s, "onChange", vec![("checked", Data::Bool(!checked))]))
}

/// Desktop's `TextInput` over the plugin's editor (`ctx:CreateEditor`). Enter fires the
/// editor's `Submitted` signal.
fn field(a: &mut NodeArgs) -> Result<El, String> {
    let editor = a.data("editor").ok_or("ui.field needs an editor from ctx:CreateEditor")?;
    let (Some(plugin), Some(index)) = (editor.get("plugin").as_str().map(str::to_owned), editor.get("id").as_i64()) else { return Err("ui.field needs an editor from ctx:CreateEditor".into()) };
    let index = index as usize;
    // The field draws its editor's text, caret and focus: the render runs again as they change.
    cx::read(cx::Dep::Editor(plugin.as_str().into(), index));
    cx::with_app(|app| {
        let editors = app.plugins.editors.borrow();
        let Some(edit) = editors.get(&(plugin.clone(), index)) else { return Ok(el()) };
        if a.string("variant") == Some("filter") {
            return Ok(filter_field(a, edit, plugin, index));
        }
        if a.string("variant") == Some("bare") {
            return Ok(bare_field(a, edit, plugin, index));
        }
        let edit_plugin = plugin.clone();
        let mut input = rune_kit::TextInput::new(edit)
            .placeholder(a.string("placeholder").unwrap_or_default().to_owned())
            .invalid(a.flag("invalid"))
            .disabled(a.flag("disabled"));
        // A read-only field draws as usual and takes no edit.
        if !a.flag("readOnly") {
            input = input.on_edit(move |app: &mut App| {
                super::editors::note_side_field(app, &edit_plugin);
                super::editors::get_mut(app, &edit_plugin, index)
            });
        }
        if a.string("w") == Some("full") {
            input = input.width(None);
        }
        if a.string("size") == Some("large") {
            input = input.large();
        } else if a.string("size") == Some("small") {
            input = input.height(24.0);
        }
        if a.flag("secure") {
            input = input.secure();
        }
        if let Some(name) = a.string("icon") {
            input = input.leading_icon(named_icon(name));
        }
        // Desktop's `showClearButton`: the button empties the field, and the editor's watch
        // reports the change to the plugin.
        if a.flag("clearable") && !a.flag("disabled") && !a.flag("readOnly") {
            let clear_plugin = plugin.clone();
            input = input.clear_button().on_action(move |app: &mut App, tx: &mut Tx| {
                tx.instant();
                super::editors::get_mut(app, &clear_plugin, index).replace_text("");
                bump();
            });
        }
        let handed = handed_keys(a);
        let focus_down = a.string("focusDown").map(str::to_owned);
        if !handed.is_empty() || focus_down.is_some() {
            let key_plugin = plugin.clone();
            input = input.on_key(move |app: &mut App, ev: &rune::KeyEvent, tx: &mut Tx| {
                // Keys during IME composition belong to the candidate window.
                if super::editors::is_composing(app, &key_plugin, index) {
                    return false;
                }
                // Down leaves the field for the list under it, as official's site search does
                // (`site-chooser.tsx:423-428`); the field gives up its focus so the request wins.
                if let Some(target) = focus_down.as_deref()
                    && ev.key == rune::Key::Down
                    && ev.mods == rune::Mods::default()
                {
                    tx.instant();
                    super::editors::get_mut(app, &key_plugin, index).focused = false;
                    tx.focus(target, true);
                    bump();
                    return true;
                }
                let Some(&(_, _, name)) = handed.iter().find(|(key, shift, _)| {
                    ev.key == *key && ev.mods.shift == *shift && !ev.mods.cmd && !ev.mods.ctrl && !ev.mods.alt
                }) else {
                    return false;
                };
                tx.instant();
                key_pressed(app, &key_plugin, index, name, tx);
                true
            });
        }
        if let Some(w) = a.number("w").filter(|w| *w > 0.0) {
            input = input.width(Some(w as f32));
        } else if a.string("w") == Some("full") {
            input = input.width(None);
        }
        // The node's `dir` reaches the value, not just the frame: `auto` starts an Arabic name at the right.
        // A plugin's field reads the way its text does unless it says otherwise (`dir="auto"`); a
        // secret reads left to right.
        input = input.dir(dir(a).unwrap_or(if a.flag("secure") { rune::Direction::Ltr } else { rune::Direction::Auto }));
        input = input.on_submit(move |app: &mut App, tx: &mut Tx| submitted(app, &plugin, index, tx));
        Ok(input.build(kit(&p(a))))
    })
    .ok_or_else(|| "ui.field draws only in a render".to_owned())?
}

/// The `bare` variant: the text, caret and selection alone at 13 / 440 (tracking -0.04), on no
/// box of its own (the browser's address input, `bar.css`). The keys `keys` names go to the
/// editor's `KeyPressed`.
fn bare_field(a: &NodeArgs, edit: &rune::TextEdit, plugin: String, index: usize) -> El {
    let pal = p(a);
    let handed = handed_keys(a);
    let (on_edit, on_change, on_submit, on_key) = (plugin.clone(), plugin.clone(), plugin.clone(), plugin);
    let font = TextStyle::new(13.0, 440.0).lh(16.0).tracking(-0.04);
    let mut input = session_view::style::field(edit, font)
        .spellcheck(false)
        .placeholder(a.string("placeholder").unwrap_or_default().to_owned(), pal.faint)
        .selection_style(pal.field_selection)
        .color(pal.text)
        .grow(1.0)
        .shrink()
        .min_w(0.0)
        .on_edit(move |app: &mut App| {
            super::editors::note_side_field(app, &on_edit);
            super::editors::get_mut(app, &on_edit, index)
        })
        .on_input_change(move |app: &mut App, tx: &mut Tx| input_changed(app, &on_change, index, tx))
        .on_submit(move |app: &mut App, tx: &mut Tx| submitted(app, &on_submit, index, tx));
    // A plugin's field reads the way its text does unless it says otherwise (`dir="auto"`); a
    // secret reads left to right.
    input = input.dir(dir(a).unwrap_or(if a.flag("secure") { rune::Direction::Ltr } else { rune::Direction::Auto }));
    if !handed.is_empty() {
        input = input.on_input_key(move |app: &mut App, ev: &rune::KeyEvent, tx: &mut Tx| {
            // Keys during IME composition belong to the candidate window (`panel.tsx:1084`).
            if super::editors::is_composing(app, &on_key, index) {
                return false;
            }
            // Note Mod+V paste without swallowing it so the next `Changed` cancels inline completion.
            if ev.key == rune::Key::Command('v') && !ev.mods.shift && !ev.mods.alt {
                super::editors::mark_pasted(&on_key, index);
                return false;
            }
            let Some(&(_, _, name)) = handed.iter().find(|(key, shift, _)| {
                ev.key == *key && ev.mods.shift == *shift && !ev.mods.cmd && !ev.mods.ctrl && !ev.mods.alt
            }) else {
                return false;
            };
            tx.instant();
            key_pressed(app, &on_key, index, name, tx);
            true
        });
    }
    input
}

/// The keys `keys` names, which the field hands to its editor's `KeyPressed`: the key, whether
/// Shift is held, and the name the plugin receives.
fn handed_keys(a: &NodeArgs) -> Vec<(rune::Key, bool, &'static str)> {
    let Some(Data::List(keys)) = a.data("keys") else { return Vec::new() };
    keys.iter()
        .filter_map(|k| match k.as_str()? {
            "ArrowUp" => Some((rune::Key::Up, false, "ArrowUp")),
            "ArrowDown" => Some((rune::Key::Down, false, "ArrowDown")),
            "Escape" => Some((rune::Key::Escape, false, "Escape")),
            "Tab" => Some((rune::Key::Tab, false, "Tab")),
            "Shift+Tab" => Some((rune::Key::Tab, true, "Shift+Tab")),
            "Shift+Delete" => Some((rune::Key::Delete, true, "Shift+Delete")),
            "Shift+Backspace" => Some((rune::Key::Backspace, true, "Shift+Backspace")),
            "Shift+Enter" => Some((rune::Key::Enter, true, "Shift+Enter")),
            _ => None,
        })
        .collect()
}

fn step_filter_highlight(list: &[String], current: Option<&str>, down: bool) -> Option<String> {
    if list.is_empty() {
        return None;
    }
    let cur = current.and_then(|p| list.iter().position(|x| x == p)).unwrap_or(0);
    let next = if down { (cur + 1).min(list.len() - 1) } else { cur.saturating_sub(1) };
    Some(list[next].clone())
}

fn clear_filter_highlight(app: &mut App, field_key: &str) {
    let Some(sid) = app.active_session_id().map(str::to_owned) else { return };
    if field_key == "files-filter" {
        if let Some(ws) = app.kernel.slots.get_mut(&crate::files::FILES).sessions.get_mut(&sid) {
            ws.highlighted_filter_path = None;
        }
    } else if field_key == "review-filter"
        && let Some(sr) = app.kernel.slots.get_mut(&crate::review::REVIEW).sessions.get_mut(&sid)
    {
        sr.highlighted_filter_path = None;
        sr.stamp += 1;
    }
}

/// The `filter` variant: a panel sidebar's filter (`review::chrome::filter_field`). Escape clears
/// it and keeps the keyboard; Up/Down move the filtered list highlight (`file/list.tsx:25-50`,
/// `review/panel.tsx:424-430`); Enter opens/selects the highlighted match.
fn filter_field(a: &NodeArgs, edit: &rune::TextEdit, plugin: String, index: usize) -> El {
    let pal = p(a);
    let placeholder = a.string("placeholder").unwrap_or(opencode_i18n::t!("ui.sessionReviewV2.filterFiles")).to_owned();
    let key = a.string("key").unwrap_or("filter").to_owned();
    let (key_sub, key_clear, key_k) = (key.clone(), key.clone(), key.clone());
    let (on_edit, on_submit, on_key, on_clear) = (plugin.clone(), plugin.clone(), plugin.clone(), plugin);
    crate::review::chrome::filter_field(
        &pal,
        &key,
        edit,
        &format!("{key}-inner"),
        &placeholder,
        move |app: &mut App| super::editors::get_mut(app, &on_edit, index),
        move |app: &mut App, tx: &mut Tx| {
            let query = super::editors::get_mut(app, &on_submit, index).text.trim().to_owned();
            if !query.is_empty()
                && let Some(sid) = app.active_session_id().map(str::to_owned)
            {
                if key_sub == "files-filter" {
                    let list = app.filtered_files(&sid, &query);
                    let pick = app
                        .kernel
                        .slots
                        .get(&crate::files::FILES)
                        .sessions
                        .get(&sid)
                        .and_then(|w| w.highlighted_filter_path.clone())
                        .filter(|p| list.contains(p))
                        .or_else(|| list.first().cloned());
                    if let Some(path) = pick {
                        tx.instant();
                        crate::files::ops::open_workspace_file_or_page(app, &path, false, tx);
                        bump();
                        return;
                    }
                } else if key_sub == "review-filter" {
                    let all: Vec<String> = app
                        .kernel
                        .slots
                        .get(&crate::review::REVIEW)
                        .sessions
                        .get(&sid)
                        .map(|r| r.meta.iter().map(|m| m.path.clone()).collect())
                        .unwrap_or_default();
                    let list = crate::review::data::filter_review_files(&all, &query);
                    let pick = app
                        .kernel
                        .slots
                        .get(&crate::review::REVIEW)
                        .sessions
                        .get(&sid)
                        .and_then(|sr| sr.highlighted_filter_path.clone())
                        .filter(|p| list.contains(p))
                        .or_else(|| list.first().cloned());
                    if let Some(path) = pick {
                        tx.instant();
                        if let Some(sr) = app.kernel.slots.get_mut(&crate::review::REVIEW).sessions.get_mut(&sid) {
                            sr.show(&path);
                            sr.tree.reveal(&path, tx);
                        }
                        bump();
                        return;
                    }
                }
            }
            submitted(app, &on_submit, index, tx);
        },
        move |app: &mut App, tx: &mut Tx| {
            tx.instant();
            let edit = super::editors::get_mut(app, &on_clear, index);
            let focused = edit.focused;
            edit.replace_text("");
            edit.focused = focused;
            clear_filter_highlight(app, &key_clear);
            bump();
        },
        // Desktop's filter (`session-review-v2.tsx`, a `type="search"` input) marks no spelling.
        false,
        move |app: &mut App, ev: &rune::KeyEvent, tx: &mut Tx| {
            // A key in the field is a key inside the side region: its tab's label wakes
            // (`FocusMode::Quiet`).
            if app.kernel.slots.get(&crate::panels::SIDE).focus.is_quiet() {
                app.kernel.slots.get_mut(&crate::panels::SIDE).focus.wake();
            }
            // The panel's tab keys from its filter (`file.tab.prev`/`next` act while the side
            // region has focus), not the field's Tab traversal.
            use crate::keymap::Bind;
            if [Bind::TabPrev, Bind::TabNext, Bind::PaneTabPrev, Bind::PaneTabNext].iter().any(|b| b.matches_tx(ev, tx)) {
                return app.panels_key(ev, tx);
            }
            if matches!(ev.key, rune::Key::Down | rune::Key::Up) && ev.mods == rune::Mods::default() {
                if super::editors::is_composing(app, &on_key, index) {
                    return false;
                }
                let query = super::editors::get_mut(app, &on_key, index).text.trim().to_owned();
                if query.is_empty() {
                    return false;
                }
                let Some(sid) = app.active_session_id().map(str::to_owned) else {
                    return false;
                };
                let down = ev.key == rune::Key::Down;
                if key_k == "files-filter" {
                    let list = app.filtered_files(&sid, &query);
                    let cur = app
                        .kernel
                        .slots
                        .get(&crate::files::FILES)
                        .sessions
                        .get(&sid)
                        .and_then(|w| w.highlighted_filter_path.clone());
                    if let Some(next) = step_filter_highlight(&list, cur.as_deref(), down) {
                        tx.instant();
                        if let Some(ws) = app.kernel.slots.get_mut(&crate::files::FILES).sessions.get_mut(&sid) {
                            ws.highlighted_filter_path = Some(next.clone());
                        }
                        tx.scroll_into_view(format!("ff:{next}"));
                        bump();
                        return true;
                    }
                } else if key_k == "review-filter" {
                    let all: Vec<String> = app
                        .kernel
                        .slots
                        .get(&crate::review::REVIEW)
                        .sessions
                        .get(&sid)
                        .map(|r| r.meta.iter().map(|m| m.path.clone()).collect())
                        .unwrap_or_default();
                    let list = crate::review::data::filter_review_files(&all, &query);
                    let cur = app
                        .kernel
                        .slots
                        .get(&crate::review::REVIEW)
                        .sessions
                        .get(&sid)
                        .and_then(|sr| sr.highlighted_filter_path.clone());
                    if let Some(next) = step_filter_highlight(&list, cur.as_deref(), down) {
                        tx.instant();
                        if let Some(sr) = app.kernel.slots.get_mut(&crate::review::REVIEW).sessions.get_mut(&sid) {
                            sr.highlighted_filter_path = Some(next.clone());
                            sr.stamp += 1;
                        }
                        tx.scroll_into_view(format!("rf:{next}"));
                        bump();
                        return true;
                    }
                }
                return false;
            }
            if ev.key != rune::Key::Escape {
                return false;
            }
            // A `type="search"` input: Escape clears it, and on an empty one keeps the keyboard.
            tx.instant();
            let edit = super::editors::get_mut(app, &on_key, index);
            if !edit.text.is_empty() {
                edit.replace_text("");
            }
            clear_filter_highlight(app, &key_k);
            bump();
            true
        },
    )
}

fn pill(a: &mut NodeArgs) -> Result<El, String> {
    let pal = p(a);
    let words = a.string("text").unwrap_or_default().to_owned();
    let key = a.string("key").unwrap_or("pill").to_owned();
    let mut chip = cx::with_app(|app| crate::subagents::chip(app, &pal, &key, words.clone(), a.flag("pressed")))
        .ok_or("ui.pill draws only in a render")?
        .shrink()
        .min_w(0.0);
    if let Some(name) = a.string("icon") {
        let ink = match a.string("tone") {
            Some(t @ ("success" | "warning" | "danger")) => session_view::palette::token(pal.dark, &format!("state-fg-{t}")).unwrap_or(pal.icon_muted),
            Some("accent") => pal.v2.icon_accent,
            _ => pal.icon_muted,
        };
        let glyph = rune::icon(named_icon(name), 14.0).color(ink);
        chip = chip.pl(8.0).gap(6.0).child(el().size(16.0, 16.0).center().child(glyph));
    }
    chip = chip.child(text(words).font(crate::view::UI).color(pal.text).truncate().max_w(260.0).shrink().min_w(0.0));
    Ok(a.on_press(chip, "onPress"))
}

fn settings_row(a: &mut NodeArgs) -> Result<El, String> {
    let pal = p(a);
    let id = a.string("id").unwrap_or_default().to_owned();
    let control = if a.children.is_empty() { el() } else { a.children.remove(0) };
    let row = crate::settings::setting(&pal, &id, a.string("title").unwrap_or_default(), a.string("description").unwrap_or_default(), control);
    let lit = cx::with_app(|app| crate::settings::search::reveal_of(app, &id)).flatten();
    Ok(crate::settings::pages::revealed(&pal, lit, 20.0, row))
}

fn settings_list(a: &mut NodeArgs) -> Result<El, String> {
    let pal = p(a);
    let key = a.string("key").unwrap_or("plugin-settings").to_owned();
    let rows = std::mem::take(&mut a.children);
    // Desktop's `SettingsList`: the app's own card, its rows split by hairlines.
    let card = if a.flag("catalog") { crate::settings::pages::catalog(&pal, &key, rows) } else { crate::settings::list(&pal, &key, rows) };
    Ok(match a.string("title") {
        Some(title) => crate::settings::pages::section(&pal, &format!("plugin-section-{title}"), Some(title), card),
        None => card,
    })
}

/// Official `Menu.SubTrigger` right-pointing triangle (`menu.tsx:53-63`: `M6 4L10 8L6 12V4Z`).
static MENU_SUB_TRIANGLE: rune::Icon = rune::Icon {
    name: "oc-menu-sub-triangle",
    view: 16.0,
    body: r#"<path d="M6 4L10 8L6 12V4Z" fill="currentColor"/>"#,
};

/// A popover row that opens popover `popover` beside itself: either a Session details service row
/// (`icon` set -> `chevron-right`) or a `Menu.SubTrigger` row (`icon` omitted -> filled `▸`
/// triangle from `menu.tsx:53-63`). The pointer coming onto it, or a click, shows its list
/// ([`crate::plugin_host::Flyout`]).
fn flyout_row(a: &mut NodeArgs) -> Result<El, String> {
    let pal = p(a);
    let words = a.string("text").unwrap_or_default().to_owned();
    let key = a.string("key").unwrap_or("flyout").to_owned();
    let plugin = cx::rendering();
    let expanded = plugin.as_ref().is_some_and(|plugin| {
        cx::read(cx::Dep::Flyout(plugin.clone()));
        cx::with_app(|app| app.plugins.overlay.flyout_row(plugin) == Some(key.as_str())).unwrap_or(false)
    });
    let lead_icon = a.string("icon").map(str::to_owned);
    let menu_style = lead_icon.is_none();
    let height = a.number("h").map_or(28.0, |n| n as f32);
    let pl = a.number("pl").map_or(12.0, |n| n as f32);
    let pr = a.number("pr").map_or(if menu_style { 6.0 } else { 8.0 }, |n| n as f32);
    let radius = a.number("radius").map_or(4.0, |n| n as f32);
    let mut r = row()
        .role("button")
        .label(words.clone())
        .w_full()
        .h(height)
        .pl(pl)
        .pr(pr)
        .gap(8.0)
        .center_y()
        .radius(radius)
        .hover_motion(rune::Anim::INSTANT, rune::Anim::INSTANT);
    // An open row's fill is the hover fill (`[data-expanded]`), which the pointer doesn't add to.
    r = if expanded { r.fill(pal.hover) } else { r.hover(pal.hover) };
    if let Some(name) = lead_icon.as_deref() {
        r = r.child(rune::icon(named_icon(name), 16.0).color(pal.icon_muted));
    }
    let font = if menu_style { crate::view::UI } else { TextStyle::new(13.0, 440.0).lh(16.0) };
    r = r.child(text(words).font(font).color(pal.text).truncate().grow(1.0).shrink().min_w(0.0));
    if a.flag("alert") {
        r = r.child(el().w(6.0).h(16.0).center().child(el().size(6.0, 6.0).radius(3.0).fill(crate::widgets::status_ink(&pal, "failed"))));
    }
    if let Some(detail) = a.string("detail").filter(|d| !d.is_empty()) {
        r = r.child(text(detail.to_owned()).font(TextStyle::new(12.0, 440.0).lh(16.0).features("\"tnum\"")).color(pal.faint).single_line());
    }
    r = if menu_style {
        r.child(rune::icon(&MENU_SUB_TRIANGLE, 16.0).color(pal.faint))
    } else {
        r.child(rune::icon(crate::plugin_host::icons::resolve("", "chevron-right"), 16.0).color(pal.icon_muted).ml(4.0))
    };
    let (Some(plugin), Some(popover)) = (plugin, a.string("popover").map(str::to_owned)) else { return Ok(r) };
    let side = match a.string("side") {
        Some("left") => rune::Side::Left,
        Some("top") => rune::Side::Top,
        Some("bottom") => rune::Side::Bottom,
        _ => rune::Side::Right,
    };
    let data = a.data("props").unwrap_or_default();
    let target = flyout::Row { id: popover, row: key.clone(), side, data };
    // Steps the popover's flyout (`flyout-machine.ts`) with the event `make` builds for this row.
    let ask = std::rc::Rc::new(move |app: &mut App, tx: &mut Tx, make: &dyn Fn(flyout::Row) -> FlyoutEvent| {
        cx::ask_in(app, &plugin, cx::Request::Flyout(make(target.clone())));
        requests::carry_out(app, tx);
    });
    let (hover, moved, press) = (ask.clone(), ask.clone(), ask);
    let (row_key, laid) = (key.clone(), key.clone());
    Ok(r.key(key)
        .on_layout_tx(move |app: &mut App, rect, tx: &mut Tx| {
            app.plugins.aim.rows.borrow_mut().insert(laid.clone(), rect);
            tx.live_only();
        })
        // Coming onto a row opens a closed flyout (`row.entered`); leaving it for the gap
        // between rows, or the rows altogether, ends an aim (`row.moved` onto no row,
        // `aim.cancelled`).
        .on_hover_change(move |app: &mut App, inside: bool, tx: &mut Tx| {
            if inside {
                hover(app, tx, &FlyoutEvent::Entered);
            } else if app.plugins.overlay.flyout_due() {
                hover(app, tx, &|_: flyout::Row| FlyoutEvent::Settled);
            } else {
                tx.live_only();
            }
        })
        // Moving over a row switches an open flyout at once, unless the pointer heads for the
        // open list across it (`row.moved`).
        .on_pointer_move_local(move |app: &mut App, e: rune::PointerEvent, tx: &mut Tx| {
            let aim = &app.plugins.aim;
            let Some(point) = aim.point(&row_key, [e.x, e.y]) else { return tx.live_only() };
            aim.step(point);
            let Some(open) = app.plugins.overlay.flyout_list().map(|l| (l.row.clone(), l.side)) else { return tx.live_only() };
            if open.0 == row_key && !app.plugins.overlay.flyout_due() {
                return tx.live_only();
            }
            let aiming = open.0 != row_key && aim.heading(point, open.1);
            let now = *app.clock;
            moved(app, tx, &move |row: flyout::Row| FlyoutEvent::Moved { row, aiming, now });
        })
        .on_press_tx(move |app: &mut App, tx: &mut Tx| press(app, tx, &FlyoutEvent::Activated)))
}

fn dialog_header(a: &mut NodeArgs) -> Result<El, String> {
    ui_nodes::dialog_header(a, |close| close.on_press_tx(|s: &mut App, tx: &mut Tx| requests::dismiss_dialog(s, tx)))
}

fn markdown(a: &mut NodeArgs) -> Result<El, String> {
    let pal = p(a);
    let source = a.string("source").unwrap_or_default().to_owned();
    let width = a.number("w").map_or_else(session_view::style::content_w, |w| w as f32);
    let ink = a.color("color").unwrap_or(pal.text);
    let mut md = session_view::style::markdown_style(&pal, ink, width);
    if let Some(lh) = a.number("lineHeight") {
        md = md.body_font(session_view::style::MD.lh(lh as f32));
    }
    let none: Vec<(&[std::ops::Range<usize>], rune_kit::Highlight)> = Vec::new();
    Ok(session_view::style::markdown_styled::<App, _>(&pal, &source, 0.0, a.flag("streaming"), md, none, Vec::new(), None, &[], None, None))
}
