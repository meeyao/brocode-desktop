//! What a host tells the pickers, and what they do with it: the selection, the lists, picking,
//! and the keys.

use opencode_kit::model::{ModelRef, capitalize};
use rune::{El, Key, KeyEvent, TextEdit, Tx};

use super::{ADD_PROJECT_KEY, Choice, Effort, FAVORITES, Item, Kind, Model, ModelAction, Pickers, RECENT, Selection, grouped, hover, menu, search, speed};
use super::hover::ModelHover;
use crate::composer::ComposerHost;
use crate::palette::Pal;
use crate::popover::{self as pop, Nav, cycle, nav};

/// Host hooks for the agent, model and effort pickers. The provided methods are the pickers'
/// behaviour; a host overrides only the hooks above them.
pub trait PickerHost: ComposerHost {
    /// The host's localized budget label and whether the model has reached its cap.
    fn model_budget(&self, _key: &str) -> Option<(String, bool)> { None }
    fn pickers(&self) -> &Pickers;
    fn pickers_mut(&mut self) -> &mut Pickers;

    /// Where picks are kept: the open session's id, a new session's draft's own id, or "draft".
    fn picker_scope(&self) -> String;
    /// A new session, which starts with what the toolbar shows.
    fn picker_draft(&self) -> bool {
        false
    }
    /// What the server last said the open session runs with (either part may be empty).
    fn picker_running(&self) -> Option<Selection>;
    /// The model when neither the session nor its agent names one.
    fn picker_fallback_model(&self) -> Option<ModelRef> {
        None
    }
    /// On a subagent's page: its session id and what it runs with. A reply to it always names
    /// its agent and model, so it carries on as itself.
    fn picker_subagent(&self) -> Option<(String, Selection)> {
        None
    }
    /// The picks kept for `scope` changed: a host that keeps them across a relaunch writes
    /// them (`selection-machine.ts` `persist.write`).
    fn picker_changed(&mut self, _scope: &str) {}
    /// Whether the toolbar shows the agent's picker.
    fn picker_show_agent(&self) -> bool {
        true
    }
    /// A key the model search field doesn't use, for the host's own shortcuts.
    fn picker_search_key(&mut self, _ev: &KeyEvent, _tx: &mut Tx) -> bool {
        false
    }

    /// Lets a live host refresh a partial list or notify plugins without delaying the dialog or its input.
    fn picker_model_opening(&mut self, _tx: &mut Tx) {}

    /// The buttons beside the model search, in order: Manage models opens here; a host that can
    /// connect a provider puts Connect first.
    fn picker_model_actions(&self) -> Vec<ModelAction> {
        vec![ModelAction::Manage]
    }
    /// Connect was pressed; the list has closed.
    fn picker_model_action(&mut self, _action: ModelAction, _tx: &mut Tx) {}
    /// Opens the unpaid free-model dialog (`DialogSelectModelUnpaid`, `#64927`) when no paid
    /// provider is connected; returns `true` when the host handled the open.
    fn picker_open_unpaid(&mut self, _tx: &mut Tx) -> bool {
        false
    }

    /// A provider's mark beside its name in the narrow window's model drawer.
    fn picker_provider_mark(&self, _p: &Pal, _provider: &str) -> Option<El> {
        None
    }

    /// The project picker's rows (a host without one lists none).
    fn picker_projects(&self) -> Vec<Item> {
        Vec::new()
    }
    fn picker_project_current(&self) -> String {
        String::new()
    }
    fn picker_choose_project(&mut self, _key: &str, _tx: &mut Tx) {}
    fn picker_project_lead(&self, _p: &Pal, _key: &str, _label: &str) -> Option<El> {
        None
    }

    /// What the session runs with now: the agent and model the server last said, or for a new
    /// session the first agent with its configured model. A model the server did not name
    /// runs with the effort it inherits (`selection-machine.ts` `selectVariant`).
    fn durable(&self) -> Selection {
        let pk = self.pickers();
        let info = self.picker_running();
        let agent = pk.effective_agent(info.as_ref().map_or("", |i| i.agent.as_str())).to_owned();
        if let Some(model) = info.map(|i| i.model).filter(|m| !m.id.is_empty()) {
            return Selection { model, agent, effort: Effort::Chosen };
        }
        let model = pk.agent_models.get(&agent).cloned().or_else(|| self.picker_fallback_model()).unwrap_or_default();
        self.with_inherited(Selection { model, agent, effort: Effort::Inherited })
    }

    fn selection(&self) -> Selection {
        match self.pickers().selections.get(&self.picker_scope()) {
            Some(pick) => self.with_inherited(pick.clone()),
            None => self.durable(),
        }
    }

    /// `pick` with the effort its model inherits now, when it chose none: the saved effort
    /// and the configured one may have changed since it was kept (`composer/selection.ts`
    /// `variant.current`).
    fn with_inherited(&self, mut pick: Selection) -> Selection {
        if pick.effort == Effort::Inherited {
            pick.model.variant = self.pickers().inherited_effort(&pick.agent, &pick.model.key(), self.picker_draft());
        }
        pick
    }

    /// What the next prompt switches the session to: a pick that differs from what it runs
    /// with. An agent switch can move the model, so the model goes with it, as Desktop sends it.
    /// A new session takes what the toolbar shows, so it starts as it says.
    fn picked_choice(&self) -> (Option<String>, Option<ModelRef>) {
        if let Some((id, info)) = self.picker_subagent() {
            let picked = self.pickers().selections.get(&id);
            let agent = picked.map(|p| p.agent.clone()).unwrap_or(info.agent);
            let model = picked.map(|p| p.model.clone()).unwrap_or(info.model);
            return (Some(agent).filter(|a| !a.is_empty()), Some(model).filter(|m| !m.id.is_empty()));
        }
        let draft = self.picker_draft();
        let picked = match self.pickers().selections.get(&self.picker_scope()) {
            Some(p) => self.with_inherited(p.clone()),
            None if draft => self.durable(),
            None => return (None, None),
        };
        let now = self.durable();
        let agent = (draft || picked.agent != now.agent).then(|| picked.agent.clone());
        let model = (agent.is_some() || picked.model != now.model).then(|| picked.model.clone()).filter(|m| !m.id.is_empty());
        (agent, model)
    }

    fn picker_items_for(&self, kind: Kind) -> Vec<Item> {
        let selection = self.selection();
        let pk = self.pickers();
        match kind {
            Kind::Model => model_items(pk),
            Kind::Agent => pk.agents.iter().map(|a| Item::new(a.clone(), capitalize(a))).collect(),
            Kind::Effort => {
                let variants = pk.models.iter().find(|m| m.id == selection.model.id && m.provider == selection.model.provider);
                std::iter::once(Item::new("default", opencode_i18n::t!("common.default")))
                    .chain(variants.into_iter().flat_map(|m| &m.variants).map(|v| Item::new(v.clone(), capitalize(v)))).collect()
            }
            // Its speeds by name, as the efforts are (`composer/model.ts` `speed.options`).
            Kind::Speed => pk.speeds(&selection.model.key()).into_iter()
                .map(|(id, _)| if id == speed::STANDARD { Item::new(id, capitalize(opencode_i18n::t!("composer.speed.standard"))) } else { Item::new(id.clone(), capitalize(&id)) }).collect(),
            Kind::Project => self.picker_projects(),
        }
    }

    fn picker_items(&self) -> Vec<Item> {
        self.pickers().open().map_or_else(Vec::new, |kind| self.picker_items_for(kind))
    }

    /// The effort the model `key` would run with if picked now: "" for its default, or none
    /// when it has no efforts to choose.
    fn model_effort(&self, key: &str) -> Option<String> {
        let pk = self.pickers();
        let m = pk.models.iter().find(|m| m.key == key).filter(|m| !m.variants.is_empty())?;
        let s = self.selection();
        // A model's speeds share its effort (`models.tsx` `getVariant`).
        let current = pk.standard_key(&s.model.key()) == key;
        let effort = pk.staged.get(key).cloned()
            .or_else(|| current.then(|| s.model.variant.clone()))
            // Any other model's is the one it would inherit (`select-dialog.tsx` `variant`).
            .unwrap_or_else(|| pk.inherited_effort(&s.agent, &m.key, self.picker_draft()));
        // A session can say "default" for no choice, as the effort trigger reads it.
        Some(if effort == "default" { String::new() } else { effort })
    }

    /// Moves the model `key`'s staged effort `step` along Default and its efforts; false when it
    /// has none.
    fn step_effort(&mut self, key: &str, step: i64) -> bool {
        let Some(now) = self.model_effort(key) else { return false };
        let Some(m) = self.pickers().models.iter().find(|m| m.key == key) else { return false };
        let options: Vec<String> = std::iter::once(String::new()).chain(m.variants.iter().cloned()).collect();
        let at = options.iter().position(|v| *v == now);
        let next = options[cycle(options.len(), at, step)].clone();
        self.pickers_mut().stage_effort(key, &next);
        true
    }

    /// Stars or unstars a model (⌘F on the highlighted row, or Favorite in its flyout).
    fn toggle_favorite(&mut self, key: &str, by_pointer: bool) {
        let pk = self.pickers_mut();
        pk.prefs.toggle_favorite(key);
        // The row moves between groups; the highlight goes with it, and typing still searches.
        // From the keys the list follows the row to its new place; a press in the flyout
        // leaves the list where it is, as a row the pointer lit is not scrolled to when the
        // list reorders (official `search-list.tsx` `setActive(key, "pointer")`, opencode#65649).
        if by_pointer { pk.point(key.to_owned()) } else { pk.activate(key.to_owned()) }
        pk.search.focused = pk.open() == Some(Kind::Model);
    }

    /// Whether the selected model has an effort to choose: Desktop shows the picker (with
    /// "Default" for no choice) for any model that has variants.
    fn has_effort(&self) -> bool {
        let m = self.selection().model;
        // brocode: API Brother has no thinking stages on its side, so its models never offer any,
        // whatever `/api/model` sends.
        if m.provider == "brocode" {
            return false;
        }
        self.pickers().models.iter().find(|x| x.id == m.id && x.provider == m.provider).is_some_and(|x| !x.variants.is_empty())
    }

    /// Whether the selected model has other speeds: its speed command is offered only then, so
    /// nothing about speeds shows for the rest (`composer/commands.tsx`).
    fn has_speeds(&self) -> bool {
        !self.pickers().speeds(&self.selection().model.key()).is_empty()
    }

    /// Whether the toolbar shows the speed's picker: not at the standard speed, so it adds nothing
    /// to the composer until the speed command chooses one (`editor.tsx`).
    fn speed_shown(&self) -> bool {
        self.has_speeds() && self.picker_current(Kind::Speed) != speed::STANDARD
    }

    fn picker_current(&self, kind: Kind) -> String {
        let s = self.selection();
        match kind {
            // The row that stands for the model at any of its speeds (`select-dialog.tsx` `current`).
            Kind::Model => self.pickers().standard_key(&s.model.key()),
            Kind::Speed => self.pickers().speed_of(&s.model.key()),
            Kind::Agent => s.agent,
            Kind::Effort => if s.model.variant.is_empty() { "default".into() } else { s.model.variant },
            Kind::Project => self.picker_project_current(),
        }
    }

    fn picker_active(&self) -> Option<String> {
        let items = self.picker_items();
        let pk = self.pickers();
        if pk.open() == Some(Kind::Model) {
            pk.active.resolve(items.iter().map(|i| i.key.as_str()))
        } else {
            pk.menu.menu_active().filter(|k| items.iter().any(|i| i.key == *k)).map(str::to_owned)
        }
    }

    fn apply_menu_effect(&mut self, effect: menu::Effect, tx: &mut Tx) {
        match effect {
            menu::Effect::None => {}
            menu::Effect::FocusModelSearch => {
                self.pickers_mut().search.focused = true;
                self.composer_edit_mut().focused = false;
            }
            menu::Effect::Focus { key, ring, scroll_option } => {
                self.composer_edit_mut().focused = false;
                if let Some(opt) = scroll_option {
                    tx.scroll_into_view_in("picker-scroll", format!("option-{opt}"));
                }
                tx.focus(key, ring);
            }
            menu::Effect::ReturnTrigger { key, ring } => {
                self.composer_edit_mut().focused = false;
                tx.return_focus(key, ring);
            }
            menu::Effect::FocusComposer => {
                self.composer_edit_mut().focused = true;
            }
            menu::Effect::LeaveFocus => {
                self.composer_edit_mut().focused = false;
            }
        }
    }

    fn open_picker_with(&mut self, kind: Kind, gesture: menu::Gesture, tx: &mut Tx) {
        let opening_model = kind == Kind::Model && self.pickers().open() != Some(Kind::Model);
        if opening_model
            && !self.pickers().has_paid_provider()
            && self.pickers().models.iter().any(|m| m.free)
            && self.picker_open_unpaid(tx)
        {
            return;
        }
        let item_keys: Vec<String> = self.picker_items_for(kind).into_iter().map(|i| i.key).collect();
        let pk = self.pickers_mut();
        let eff = menu::step(&mut pk.menu, menu::Event::Open { kind, gesture, items: &item_keys });
        pk.search = TextEdit::new("");
        pk.search.focused = false;
        pk.staged.clear();
        pk.hover = hover::Hover::fresh();
        if pk.open() == Some(Kind::Model) {
            pk.active.clear();
        }
        self.apply_menu_effect(eff, tx);
        if opening_model {
            self.picker_model_opening(tx);
        }
        if self.pickers().open() == Some(Kind::Model) && let Some(key) = self.picker_active() {
            self.flyout_event(hover::machine::Event::Highlight { id: &key, option: None }, tx);
        }
    }

    fn open_picker(&mut self, kind: Kind) {
        self.open_picker_with(kind, menu::Gesture::Keyboard(menu::EntryFocus::First), &mut Tx::default());
    }

    fn close_picker_with(&mut self, reason: menu::CloseReason, tx: &mut Tx) {
        let pk = self.pickers_mut();
        let eff = menu::step(&mut pk.menu, menu::Event::Close(reason));
        pk.hover = hover::Hover::fresh();
        pk.search.focused = false;
        pk.search.cancel_composition();
        pk.search.break_history_group();
        self.apply_menu_effect(eff, tx);
    }

    fn close_picker(&mut self, restore: bool) {
        let reason = if restore { menu::CloseReason::Escape } else { menu::CloseReason::Replaced };
        self.close_picker_with(reason, &mut Tx::default());
    }

    fn hover_picker_option(&mut self, key: String, tx: &mut Tx) {
        let eff = menu::step(&mut self.pickers_mut().menu, menu::Event::Hover(key));
        self.apply_menu_effect(eff, tx);
    }

    fn apply_selection_for(&mut self, kind: Kind, key: &str) -> bool {
        let mut selected = self.selection();
        let session = self.picker_scope();
        let draft = self.picker_draft();
        // An open session's selection owns the effort it shows, whatever is saved or
        // configured later (`selection.tsx` `set`); a draft's goes on inheriting until one is
        // chosen (`composer/selection.ts` `set`).
        if !draft { selected.effort = Effort::Chosen; }
        // What an agent that names no model runs with (`composer/selection.ts` `key`).
        let fallback = (kind == Kind::Agent).then(|| self.picker_fallback_model()).flatten();
        let pk = self.pickers_mut();
        match kind {
            Kind::Model => {
                // A model runs at the speed last used with it. Another speed of the same model
                // keeps what the selection holds; another model starts with no effort chosen
                // (`composer/selection.ts` `set`).
                let key = &pk.standard_key(key);
                let target = pk.remembered_speed(key);
                let Some(m) = pk.models.iter().find(|m| m.key == target).cloned() else { return false };
                pk.prefs.picked(key);
                let same = pk.standard_key(&selected.model.key()) == *key;
                selected.model.id = m.id.clone();
                selected.model.provider = m.provider.clone();
                let staged = pk.staged.get(key).cloned();
                match staged {
                    Some(staged) => {
                        pk.prefs.set_variant(key, &staged);
                        selected.model.variant = staged;
                        selected.effort = Effort::Chosen;
                    }
                    None if same => {}
                    None => selected.effort = Effort::Inherited,
                }
            }
            Kind::Agent => {
                // Picking the agent already shown changes nothing (`selection-machine.ts`
                // `editSelection`).
                if selected.agent == key { return true; }
                // In an open session the agent it leaves keeps the model and effort it showed,
                // for when it is picked again (`selection-machine.ts` `editSelection`). A draft
                // keeps only a model that was picked for the agent, which the pick remembered
                // (`composer/selection.ts`: the agent's `prompt.model`, none when nothing was).
                if !draft {
                    let left = Choice { model: selected.model.clone(), effort: Effort::Chosen };
                    pk.choices.entry(session.clone()).or_default().insert(selected.agent.clone(), left);
                }
                match pk.choices.get(&session).and_then(|c| c.get(key)).cloned() {
                    // The model the agent last had here comes back as it was. One a draft
                    // remembered has no effort of its own, also in the session the draft
                    // started (`composer/selection.ts` `remembered`).
                    Some(c) => {
                        selected.model = c.model;
                        selected.effort = if draft { Effort::Inherited } else { c.effort };
                    }
                    // An agent new here runs with its own model, else with the one a session
                    // nobody chose a model for starts with: the config's, a recent one or the
                    // first (`composer/selection.ts` `key`, `selection.tsx` `key`). The model
                    // on screen was the other agent's, so it does not stay. No effort is chosen.
                    None => {
                        if let Some(m) = pk.agent_models.get(key).cloned().or(fallback) {
                            selected.model = m;
                            selected.effort = Effort::Inherited;
                        }
                    }
                }
                selected.agent = key.into();
            }
            Kind::Effort => {
                selected.model.variant = if key == "default" { String::new() } else { key.into() };
                selected.effort = Effort::Chosen;
                let standard = pk.standard_key(&selected.model.key());
                pk.prefs.set_variant(&standard, &selected.model.variant);
            }
            // The same model at another speed, remembered for when it is picked again
            // (`composer/selection.ts` `speed.set`). A chosen effort goes with it; an inherited
            // one is asked again for the speed it moves to (opencode#66107).
            Kind::Speed => {
                let now = selected.model.key();
                let Some((_, target)) = pk.speeds(&now).into_iter().find(|(id, _)| id == key) else { return false };
                let Some(m) = pk.models.iter().find(|m| m.key == target) else { return false };
                selected.model.id = m.id.clone();
                pk.prefs.set_speed(&pk.standard_key(&now), Some(key).filter(|k| *k != speed::STANDARD));
            }
            Kind::Project => return false,
        }
        if selected.effort == Effort::Inherited {
            selected.model.variant = pk.inherited_effort(&selected.agent, &selected.model.key(), draft);
            // An open session keeps what that came to (`selection.tsx` `set`: "A session
            // selection owns its variant even when preferences change in another session").
            if !draft { selected.effort = Effort::Chosen; }
        }
        if draft && kind != Kind::Agent { pk.remember_draft_pick(&session, &selected); }
        let live = !self.picker_draft();
        let durable = self.durable();
        let pk = self.pickers_mut();
        if live && selected == durable { pk.selections.remove(&session); } else { pk.selections.insert(session.clone(), selected); }
        self.picker_changed(&session);
        true
    }

    fn choose_picker(&mut self, key: &str) {
        self.choose_picker_tx(key, &mut Tx::default());
    }

    fn choose_picker_tx(&mut self, key: &str, tx: &mut Tx) {
        let Some(kind) = self.pickers().open() else { return };
        // Never accept a stale, filtered-out item or a non-option (including empty results).
        if !self.picker_items().iter().any(|i| i.key == key) { return; }
        if kind == Kind::Project {
            if key != ADD_PROJECT_KEY { self.close_picker_with(menu::CloseReason::Picked, tx); }
            self.picker_choose_project(key, tx);
            return;
        }
        if !self.apply_selection_for(kind, key) { return; }
        tx.color_motion(rune::Spring::NONE);
        self.close_picker_with(menu::CloseReason::Picked, tx);
    }

    /// A picker's keys. Opening it (unless it has an entrance to play) and moving in it land in
    /// the keystroke. (Not closing: an instant unmount of the focused search field drops the focus
    /// handed back to the composer.)
    fn picker_key(&mut self, ev: &KeyEvent, tx: &mut Tx) -> bool {
        let was = self.pickers().open();
        let used = picker_keys(self, ev, tx);
        if used && self.pickers().open().is_some() {
            let open = self.pickers().open();
            if open == was || open == Some(Kind::Model) { tx.instant(); } else { pop::motion::opening(self.knobs(), tx); }
        } else if used && was.is_some() {
            // The trigger lets go of its open look at once, as Desktop's button-v2 has no transition.
            tx.color_motion(rune::Spring::NONE);
        }
        used
    }

    /// ⌘. and ⇧⌘D: the next (or previous) agent or variant, picked directly without opening the
    /// menu or moving keyboard focus (`composer/commands.tsx:152-160`).
    fn cycle_picker(&mut self, kind: Kind, backwards: bool, _tx: &mut Tx) {
        let items = self.picker_items_for(kind);
        if items.is_empty() { return; }
        let cur = self.picker_current(kind);
        let at = items.iter().position(|i| i.key == cur);
        let next = items[cycle(items.len(), at, if backwards { -1 } else { 1 })].key.clone();
        self.apply_selection_for(kind, &next);
    }
}

/// The model list, as Desktop's (`select-dialog.tsx:561-585`, `:654-665`): with no search,
/// Favorites then Recent lead it, each only when it has a model (with their provider after the
/// name), and their models leave their provider's group, so none is listed twice. Recent is
/// the models picked lately, nothing more.
///
/// A search lists what it finds, best first and ties by name as Desktop lists them, each
/// provider's models together: [`search::score`] finds a typo as well as a word (t1389).
///
/// With no search, each provider's shown models go by name and the providers in Desktop's
/// order (`select-dialog.tsx:85-98`, `:496`): its popular ones first, then the rest by name.
fn model_items(pk: &Pickers) -> Vec<Item> {
    // Rows are grouped by the provider's id; the header says its name.
    let item = |m: &Model, group: &str, heading: &str, note: bool| Item {
        key: m.key.clone(), label: m.name.clone(), group: group.to_owned(), heading: heading.to_owned(),
        note: if note { m.group.clone() } else { String::new() }, free: m.free, latest: m.latest,
    };
    if !pk.search.text.trim().is_empty() {
        // A model's speeds are not rows of their own (`models.tsx` `listed`).
        let mut hits: Vec<(i64, Item)> = pk.models.iter()
            .filter(|m| m.speed.is_none() && pk.visible(m))
            .filter_map(|m| {
                let fields = [m.name.as_str(), m.group.as_str(), m.provider.as_str(), m.id.as_str()];
                search::score(&pk.search.text, &fields).map(|score| (score, item(m, &m.provider, &m.group, false)))
            })
            .collect();
        hits.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| rune::text::compare_names(&a.1.label, &b.1.label)));
        return grouped(hits.into_iter().map(|(_, item)| item).collect());
    }
    let prefs = &pk.prefs;
    let find = |k: &String| pk.models.iter().find(|m| &m.key == k);
    // Saved entries may name a model at any of its speeds; the lists show each model once
    // (`models.tsx` `recent`, `favorite`).
    let once = |keys: &[String]| keys.iter().map(|k| pk.standard_key(k)).fold(Vec::new(), |mut out: Vec<String>, k| { if !out.contains(&k) { out.push(k); } out });
    let (favorites, recent) = (once(&prefs.favorites), once(&prefs.recent));
    let recent: Vec<&Model> = recent.iter().filter(|k| !favorites.contains(k)).filter_map(find).collect();
    let favorites: Vec<&Model> = favorites.iter().filter_map(find).collect();
    let lead = |m: &Model| favorites.iter().chain(&recent).any(|x| x.key == m.key);
    let mut rest: Vec<&Model> = pk.models.iter().filter(|m| m.speed.is_none() && pk.visible(m) && !lead(m)).collect();
    rest.sort_by(|a, b| provider_rank(a).cmp(&provider_rank(b)).then_with(|| rune::text::compare_names(&a.name, &b.name)));
    grouped(favorites.iter().map(|m| item(m, FAVORITES, opencode_i18n::t!("dialog.model.favorites"), true))
        .chain(recent.iter().map(|m| item(m, RECENT, opencode_i18n::t!("dialog.model.recent"), true)))
        .chain(rest.into_iter().map(|m| item(m, &m.provider, &m.group, false)))
        .collect())
}

/// Desktop's `popularProviders` (`providers/catalog/order.ts`), which lead the list in this order.
// `packages/app/src/providers/catalog/order.ts` (`opencode#66736`).
const POPULAR: [&str; 9] = ["opencode-go", "opencode", "openai", "anthropic", "claude-agent", "github-copilot", "google", "openrouter", "vercel"];

/// Where a model's provider goes: a popular one by its place, the rest after them by name.
pub(super) fn provider_rank(m: &Model) -> (usize, String) {
    match POPULAR.iter().position(|p| *p == m.provider) {
        Some(at) => (at, String::new()),
        None => (POPULAR.len(), m.group.to_lowercase()),
    }
}

fn picker_keys<S: PickerHost>(s: &mut S, ev: &KeyEvent, tx: &mut Tx) -> bool {
    if let Some(open) = s.pickers().open() {
        if open == Kind::Model && flyout_key(s, ev, tx) { return true; }
        let is_space = matches!(&ev.key, Key::Text(t) if t == " ") && ev.mods == Default::default();
        match nav(ev) {
            Some(Nav::Close) => s.close_picker_with(menu::CloseReason::Escape, tx),
            // In the model dialog Tab moves on to Connect and Manage, as Desktop's does.
            Some(Nav::Leave) if open == Kind::Model => return false,
            Some(Nav::Leave) => s.close_picker_with(menu::CloseReason::Tab, tx),
            Some(Nav::Step(step)) => {
                if s.pickers().search.is_composing() { return false; }
                let items = s.picker_items();
                if !items.is_empty() {
                    if open == Kind::Model {
                        let active = s.picker_active();
                        let at = items.iter().position(|i| Some(&i.key) == active.as_ref());
                        let key = items[cycle(items.len(), at, step)].key.clone();
                        s.flyout_event(hover::machine::Event::Highlight { id: &key, option: None }, tx);
                        s.pickers_mut().hover.navigated = true;
                        tx.scroll_into_view_in("picker-scroll", format!("option-{key}"));
                        s.pickers_mut().activate(key);
                    } else {
                        let keys: Vec<String> = items.into_iter().map(|i| i.key).collect();
                        let eff = menu::step(&mut s.pickers_mut().menu, menu::Event::Step { step, items: &keys });
                        s.apply_menu_effect(eff, tx);
                    }
                }
            }
            Some(Nav::Choose) => {
                if s.pickers().search.preedit().is_none() && let Some(key) = s.picker_active() { s.choose_picker_tx(&key, tx); }
            }
            None if is_space && open != Kind::Model => {
                if let Some(key) = s.picker_active() { s.choose_picker_tx(&key, tx); }
            }
            None if matches!(ev.key, Key::Home | Key::End) && ev.mods == Default::default() && open != Kind::Model => {
                let keys: Vec<String> = s.picker_items().into_iter().map(|i| i.key).collect();
                let eff = menu::step(&mut s.pickers_mut().menu, menu::Event::Jump { last: ev.key == Key::End, items: &keys });
                s.apply_menu_effect(eff, tx);
            }
            None if crate::keymap::Bind::Favorite.matches(ev) && open == Kind::Model => {
                if let Some(key) = s.picker_active() { s.toggle_favorite(&key, false); }
                s.model_list_keyed();
            }
            // As in Desktop's list: ← → change the lit model's effort, or while searching edit
            // the query until the arrows have moved through the results (`select-dialog.tsx:897-908`).
            None if matches!(ev.key, Key::Left | Key::Right) && ev.mods == Default::default()
                && open == Kind::Model && (s.pickers().search.text.is_empty() || s.pickers().hover.navigated) => {
                let Some(key) = s.picker_active() else { return false };
                // Right to left, ← is forward (`select-dialog.tsx:904`).
                let step = match (ev.key == Key::Right, opencode_i18n::direction()) {
                    (true, opencode_i18n::Direction::Ltr) | (false, opencode_i18n::Direction::Rtl) => 1,
                    (false, opencode_i18n::Direction::Ltr) | (true, opencode_i18n::Direction::Rtl) => -1,
                };
                if step < 0 {
                    if s.step_effort(&key, step) {
                        s.model_list_keyed();
                        return true;
                    }
                    return false;
                }
                s.flyout_event(hover::machine::Event::Highlight { id: &key, option: Some(0) }, tx);
            }
            // Add project (`project.open`, which Desktop binds while its location dialog shows).
            None if open == Kind::Project && crate::keymap::Bind::OpenProject.matches_tx(ev, tx) => s.picker_choose_project(ADD_PROJECT_KEY, tx),
            None => return open != Kind::Model,
        }
        return true;
    }
    // A focused trigger opens its list on the arrows too, as Kobalte's `MenuTrigger.onKeyDown`
    // does (`ArrowDown` focuses the first row, `ArrowUp` the last).
    let toolbar_focus = [Kind::Agent, Kind::Model, Kind::Effort, Kind::Speed].into_iter().find(|k| tx.focused() == Some(k.key()));
    if matches!(ev.key, Key::Down | Key::Up) && ev.mods == Default::default()
        && let Some(kind) = toolbar_focus {
        let entry = if ev.key == Key::Up { menu::EntryFocus::Last } else { menu::EntryFocus::First };
        s.open_picker_with(kind, menu::Gesture::Keyboard(entry), tx);
        return true;
    }
    use crate::keymap::Bind;
    if Bind::Model.matches_tx(ev, tx) { s.open_picker_with(Kind::Model, menu::Gesture::Keyboard(menu::EntryFocus::First), tx); return true; }
    let cycle = match () {
        _ if Bind::Agent.matches_tx(ev, tx) => Some((Kind::Agent, false)),
        _ if Bind::AgentBack.matches_tx(ev, tx) => Some((Kind::Agent, true)),
        _ if Bind::Variant.matches_tx(ev, tx) && s.has_effort() => Some((Kind::Effort, false)),
        _ => None,
    };
    if let Some((kind, backwards)) = cycle {
        s.cycle_picker(kind, backwards, tx);
        return true;
    }
    false
}

fn flyout_key<S: PickerHost>(s: &mut S, ev: &KeyEvent, tx: &mut Tx) -> bool {
    use super::flyout::EffortFlyout;
    let Some(index) = s.pickers().hover.option() else { return false };
    let Some(id) = s.pickers().hover.flyout_model().map(str::to_owned) else { return false };
    let Some(model) = s.pickers().models.iter().find(|m| m.key == id) else { return false };
    let count = model.variants.len() + 2;
    let backward = if opencode_i18n::direction() == opencode_i18n::Direction::Ltr { Key::Left } else { Key::Right };
    let option = match &ev.key {
        Key::Down => Some((index + 1) % count),
        Key::Up => Some((index + count - 1) % count),
        key if *key == backward || *key == Key::Escape => None,
        Key::Enter => {
            if index == count - 1 { s.toggle_favorite(&id, false); s.model_flyout_dismissed(tx); }
            else {
                let effort = index.checked_sub(1).and_then(|i| model.variants.get(i)).cloned().unwrap_or_default();
                s.choose_effort(&id, &effort, tx);
            }
            return true;
        }
        _ => return false,
    };
    s.flyout_event(hover::machine::Event::Highlight { id: &id, option }, tx);
    true
}
