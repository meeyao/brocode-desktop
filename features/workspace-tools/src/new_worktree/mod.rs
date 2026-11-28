//! A new session in a new worktree, as Desktop's new-session page makes one: New worktree in
//! the location dialog's worktree step (`new-session/location/dialog.tsx`), the branch it
//! starts from beside it (`new-session/workspace/selector.tsx`), and on Send the worktree
//! (`POST /api/worktree` from the project's checkout, `workspaces/create.ts`) and then the
//! session in it (`new-session/composer-adapter.ts`).

mod typeahead;

use std::collections::HashMap;

use opencode_kit::client::{Prompt, Request, Write};
use opencode_kit::model::{Project, project_git, project_sandboxes};
use opencode_kit::path;
use opencode_search::query::{Highlight, HighlightEvent};
use opencode_ui::focus::FocusExt as _;
use opencode_ui::keymap::Bind;
use rune::{Align, Color, El, Key, KeyEvent, Side, TextEdit, Transition, Tx, col, el, icon, row, text};
use rune_theme_opencode::icons as ic;
use opencode_ui::palette::{Elevate as _, Pal};
use opencode_ui::style::UI;

/// Which checkout Settings › Worktree says a new draft of the current project uses
/// (`workspaces.worktree`); a draft in the server's default directory uses its folder.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DraftWorktree {
    #[default]
    Default,
    New,
    LastUsed,
}

/// Object-safe host boundary for new-worktree draft selection, worktree inventory, and the branch picker.
pub trait NewWorktreeHost {
    fn new_worktree(&self) -> &NewWorktree;
    fn new_worktree_mut(&mut self) -> &mut NewWorktree;

    /// Active draft tab ID when the window is on a draft route; `None` otherwise.
    fn draft_id(&self) -> Option<&str>;
    /// Current draft directory (or last picked default directory off a draft).
    fn draft_dir(&self) -> &str;
    /// Updates the draft directory (`App::set_draft_dir`).
    fn set_draft_dir(&mut self, directory: String);
    /// Project owning the active draft, if any.
    fn draft_project(&self) -> Option<&Project>;

    fn projects(&self) -> &[Project];
    fn project(&self, id: &str) -> Option<&Project>;
    fn project_at(&self, dir: &str) -> Option<&Project>;
    fn touch_project(&mut self, id: &str);

    fn checkout_branch(&self, _dir: &str) -> Option<&str> { None }
    fn settings_open(&self) -> bool { false }
    fn project_switcher_open(&self) -> bool { false }
    fn is_stage(&self) -> bool { false }

    fn set_composer_focused(&mut self, focused: bool);
    fn open_git_location(&mut self, base: bool, tx: &mut Tx);
    fn mark_worktree_onboarding_used(&mut self) {}
    fn invalidate_session_search(&mut self) {}

    fn draft_worktree(&self) -> DraftWorktree { DraftWorktree::Default }
    fn last_used_destination(&self, _project_id: &str) -> Option<&str> { None }
    fn set_last_used_destination(&mut self, _project_id: String, _place: &str) {}

    /// Dispatches a server read request (`Worktrees`, `Projects`, `Branches`) when live.
    fn send_request(&self, req: Request);
}

#[derive(Default)]
pub struct NewWorktree {
    /// Drafts that start in a new worktree, by tab id, with the branch picked to start it from
    /// (`None`: the checkout's current branch).
    drafts: HashMap<String, Option<String>>,
    /// Each project's worktree inventory (`/api/worktree`), its own checkout left out.
    listed: HashMap<String, Vec<String>>,
    /// Those of them OpenCode made, with a `strategy` (`managedWorkspaceDirectories`).
    managed: HashMap<String, Vec<String>>,
    /// The existing worktree a draft of each project last ran in, where Cycle session location
    /// goes after New worktree (`state.existing` in `workspace/controller.ts`).
    existing: HashMap<String, String>,
    pub picker: BranchPicker,
}

/// Where the open draft's session starts, as `cycleNewSessionWorktree` steps through it.
#[derive(Clone, Debug, PartialEq)]
enum Place {
    /// The project's own checkout (`main`).
    Checkout,
    /// A new worktree (`create`).
    New,
    Existing(String),
}

/// The branch a new worktree starts from: Desktop's `Typeahead` in a popover under the
/// `from <branch>` trigger (`new-session/workspace/branch-machine.ts`, `selector.tsx`). Every
/// open revalidates the checkout's list while the last one stays on screen; each typed query
/// asks the server (`GET /api/vcs/branch`, 50 at most), so a branch past the first 50 is found
/// by typing ([`typeahead`]). Escape or a pick hands focus back to the composer; a press
/// outside leaves it where it landed.
#[derive(Default)]
pub struct BranchPicker {
    popover: Popover,
    pub search: TextEdit,
    /// The checkout whose branches it lists.
    directory: String,
    /// The checkout's whole list as last listed (`initial`), for the next open.
    listed: Option<Vec<String>>,
    typeahead: typeahead::Typeahead,
    /// The lit row: the first until one's chosen, held while it's listed (`controller.ts`).
    active: Highlight,
}

/// `BranchPickerState`'s `popover`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Popover {
    #[default]
    Closed,
    Open,
}

/// Why the picker closed: Escape and a pick give the composer focus (`focus.composer`), a press
/// outside doesn't (`popover: "outside"`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Close {
    Keys,
    Outside,
}

impl BranchPicker {
    pub fn is_open(&self) -> bool {
        self.popover == Popover::Open
    }

    /// The rows, and whether they are stale (dimmed while a query the rows don't match waits).
    fn rows(&self) -> (Vec<String>, bool) {
        (self.typeahead.shown.clone(), self.typeahead.stale)
    }
}

const TRIGGER: &str = "location-branch";
// NUL is not valid in a git ref, so the checkout row has its own stable typeahead key.
const CHECKOUT: &str = "\0checkout";

/// The open draft starts its session in a new worktree.
pub fn draft_creates(host: &dyn NewWorktreeHost) -> bool {
    host.draft_id().is_some_and(|id| host.new_worktree().drafts.contains_key(id))
}

/// The branch the open draft's new worktree starts from, once one is picked.
pub fn draft_base(host: &dyn NewWorktreeHost) -> Option<&str> {
    let id = host.draft_id()?;
    host.new_worktree().drafts.get(id)?.as_deref()
}

pub fn set_draft_base(host: &mut dyn NewWorktreeHost, base: Option<String>) {
    let Some(id) = host.draft_id().map(str::to_owned) else { return };
    if let Some(selected) = host.new_worktree_mut().drafts.get_mut(&id) {
        *selected = base;
    }
}

/// New worktree in the location dialog: the session will start in a new worktree of
/// `project`, from its checkout's current branch.
pub fn choose_new_worktree(host: &mut dyn NewWorktreeHost, project: &str, directory: String) {
    host.set_draft_dir(directory);
    host.mark_worktree_onboarding_used();
    if let Some(id) = host.draft_id().map(str::to_owned) {
        host.new_worktree_mut().drafts.insert(id, None);
    }
    let dir = host.draft_dir().to_owned();
    note_destination(host, &dir, "workspace");
    host.touch_project(project);
}

/// The new-session page's location keys, while a draft is open and no dialog is:
/// Choose base branch (`selector.tsx`, a new worktree's), Choose worktree (`view.tsx`)
/// and Cycle session location (`new-session/commands.tsx`), each only where Desktop
/// enables it.
pub fn draft_location_key(host: &mut dyn NewWorktreeHost, ev: &KeyEvent, tx: &mut Tx) -> bool {
    if host.draft_id().is_none() || host.settings_open() || host.project_switcher_open() {
        return false;
    }
    let Some(project) = host.draft_project().filter(|p| project_git(&p.id)).map(|p| (p.id.clone(), p.directory.clone())) else {
        return false;
    };
    let base = Bind::BranchChoose.matches_tx(ev, tx);
    if base || Bind::LocationSelect.matches_tx(ev, tx) {
        host.open_git_location(base, tx);
        return true;
    }
    if Bind::LocationCycle.matches_tx(ev, tx) {
        tx.instant();
        cycle_location(host, &project.0, project.1);
        return true;
    }
    false
}

/// The project's checkout, then New worktree, then the worktree last used if the inventory
/// still has it, then the checkout again (`cycleNewSessionWorktree`).
fn cycle_location(host: &mut dyn NewWorktreeHost, project: &str, root: String) {
    let place = draft_place(host, project, &root);
    if let Place::Existing(dir) = &place {
        note_existing_worktree(host, project, dir.clone());
    }
    let existing = host
        .new_worktree()
        .existing
        .get(project)
        .filter(|d| project_worktrees(host, project, &root).iter().any(|w| path::trim(w) == path::trim(d)))
        .cloned();
    // A new target closes the picker (`retarget`).
    if host.new_worktree().picker.is_open() {
        let picker = &mut host.new_worktree_mut().picker;
        picker.popover = Popover::Closed;
        picker.search.focused = false;
    }
    match (place, existing) {
        (Place::Checkout, _) => choose_new_worktree(host, project, root),
        (Place::New, Some(dir)) => host.set_draft_dir(dir),
        (Place::New, None) | (Place::Existing(_), _) => host.set_draft_dir(root),
    }
}

fn draft_place(host: &dyn NewWorktreeHost, project: &str, root: &str) -> Place {
    let dir = host.draft_dir();
    if draft_creates(host) {
        return Place::New;
    }
    if project_worktrees(host, project, root).iter().any(|w| path::trim(w) == path::trim(dir)) {
        return Place::Existing(dir.to_owned());
    }
    Place::Checkout
}

/// A draft of `project` runs in its existing worktree `dir`: Cycle session location comes
/// back to it after New worktree.
pub fn note_existing_worktree(host: &mut dyn NewWorktreeHost, project: &str, dir: String) {
    host.new_worktree_mut().existing.insert(project.to_owned(), dir);
}

/// Remembers where a session of the project at `dir` last started (`workspaces.lastUsed`).
pub fn note_destination(host: &mut dyn NewWorktreeHost, dir: &str, place: &str) {
    let Some(id) = host.project_at(dir).map(|p| p.id.clone()) else { return };
    if host.last_used_destination(&id) != Some(place) {
        host.set_last_used_destination(id, place);
    }
}

/// A new draft starts where Settings › Worktree says: its folder, a new worktree, or wherever
/// its project's last session started.
pub fn apply_default_destination(host: &mut dyn NewWorktreeHost) {
    let Some(id) = host.draft_id().map(str::to_owned) else { return };
    let dir = host.draft_dir().to_owned();
    let Some(project_id) = host.project_at(&dir).map(|p| p.id.clone()) else { return };
    let new = match host.draft_worktree() {
        DraftWorktree::New => true,
        DraftWorktree::Default => false,
        DraftWorktree::LastUsed => host.last_used_destination(&project_id) == Some("workspace"),
    };
    if new {
        host.new_worktree_mut().drafts.insert(id, None);
    }
}

/// The open draft goes back to starting where its folder is.
pub fn forget_new_worktree(host: &mut dyn NewWorktreeHost) {
    if let Some(id) = host.draft_id().map(str::to_owned) {
        host.new_worktree_mut().drafts.remove(&id);
    }
}

/// The write that starts session `session` from the open draft: in a new worktree of its
/// project when the draft says so, else in its folder.
pub fn start_write(host: &dyn NewWorktreeHost, session: String, prompt: Prompt) -> Write {
    match host.draft_project().filter(|_| draft_creates(host)) {
        Some(project) => Write::StartInNewWorktree {
            session,
            project: project.id.clone(),
            from: project.directory.clone(),
            branch: draft_base(host).map(str::to_owned),
            prompt,
        },
        None => Write::StartAs { session, directory: host.draft_dir().to_owned(), prompt },
    }
}

/// Every folder other than its checkout that a session of `project` can run in: the
/// server's inventory, then the project's sandboxes (`workspaceSelectionDirectories`).
pub fn project_worktrees(host: &dyn NewWorktreeHost, project: &str, root: &str) -> Vec<String> {
    let listed = host.new_worktree().listed.get(project).cloned().unwrap_or_default();
    let mut out: Vec<String> = Vec::new();
    for dir in listed.into_iter().chain(project_sandboxes(project)) {
        if !path::same(path::trim(&dir), path::trim(root)) && !out.iter().any(|d| path::same(path::trim(d), path::trim(&dir))) {
            out.push(dir);
        }
    }
    out
}

/// The worktrees OpenCode made for `project`, as last listed.
pub fn managed_worktrees(host: &dyn NewWorktreeHost, project: &str) -> Vec<String> {
    host.new_worktree().managed.get(project).cloned().unwrap_or_default()
}

/// Membership doesn't need the picker's owned, deduplicated list.
pub fn has_worktree_at(host: &dyn NewWorktreeHost, project: &Project, dir: &str) -> bool {
    let root = path::trim(&project.directory);
    let dir = path::trim(dir.trim());
    let matches = |s: &String| path::same(path::trim(s.trim()), dir) && !path::same(path::trim(s.trim()), root);
    host.new_worktree().listed.get(&project.id).is_some_and(|dirs| dirs.iter().any(matches))
        || project_sandboxes(&project.id).iter().any(matches)
}

/// Asks the server for `project`'s worktrees (the worktree step opening, or one changing).
pub fn list_worktrees(host: &dyn NewWorktreeHost, project: &str) {
    let Some(root) = host.project(project).map(|p| p.directory.clone()) else { return };
    host.send_request(Request::Worktrees { project: project.to_owned(), root });
}

/// Asks for the worktrees of each Git project not listed yet, so a folder in one resolves
/// to its project (`project_at`), as Desktop's location lookup does.
pub fn list_unlisted_worktrees(host: &dyn NewWorktreeHost) {
    for p in host.projects().iter().filter(|p| project_git(&p.id) && !host.new_worktree().listed.contains_key(&p.id)) {
        list_worktrees(host, &p.id);
    }
}

pub fn worktrees_listed(host: &mut dyn NewWorktreeHost, project: String, directories: Vec<String>, managed: Vec<String>) {
    host.new_worktree_mut().managed.insert(project.clone(), managed);
    host.new_worktree_mut().listed.insert(project, directories);
    host.invalidate_session_search();
}

pub fn worktree_updated(host: &dyn NewWorktreeHost, project: &str) {
    list_worktrees(host, project);
    host.send_request(Request::Projects);
}

pub fn branches_listed(host: &mut dyn NewWorktreeHost, directory: &str, search: &str, branches: Option<Vec<String>>) -> bool {
    let picker = &mut host.new_worktree_mut().picker;
    if directory != picker.directory {
        return false;
    }
    picker.typeahead.step(typeahead::Event::Answered { query: search, items: branches.clone() });
    if search.is_empty() && let Some(list) = branches {
        picker.typeahead.step(typeahead::Event::Synced(&list));
        picker.listed = Some(list);
    }
    let keys: Vec<&str> = picker.typeahead.shown.iter().map(String::as_str).collect();
    picker.active.step(HighlightEvent::Rows(&keys));
    picker.is_open()
}

/// Opens the branch picker on the open draft's checkout.
pub fn open_branch_picker(host: &mut dyn NewWorktreeHost, tx: &mut Tx) {
    tx.instant();
    let directory = host.draft_dir().to_owned();
    let (initial, effects) = {
        let picker = &mut host.new_worktree_mut().picker;
        // A new checkout drops the last one's list (`retarget`).
        if picker.directory != directory {
            picker.listed = None;
            picker.directory = directory;
        }
        picker.popover = Popover::Open;
        picker.search = TextEdit::new("");
        picker.search.focused = true;
        picker.active.clear();
        let initial = picker.listed.clone();
        let effects = picker.typeahead.step(typeahead::Event::Opened { initial: initial.as_deref(), failed: false, now: tx.clock() });
        (initial, effects)
    };
    host.set_composer_focused(false);
    branch_effects(host, effects, tx);
    // Every open revalidates the list, keeping the last on screen meanwhile (`branch-machine.ts:159-172`).
    if initial.is_some() {
        ask_branches(host, String::new());
    }
}

pub fn close_branch_picker(host: &mut dyn NewWorktreeHost, tx: &mut Tx) {
    close_branch_picker_for(host, Close::Keys, tx);
}

fn close_branch_picker_for(host: &mut dyn NewWorktreeHost, why: Close, tx: &mut Tx) {
    tx.instant();
    {
        let picker = &mut host.new_worktree_mut().picker;
        picker.popover = Popover::Closed;
        picker.search.focused = false;
    }
    if why == Close::Keys && !host.is_stage() {
        host.set_composer_focused(true);
    }
}

/// The typeahead's timers.
pub fn poll_branch_picker(host: &mut dyn NewWorktreeHost, now: f64, tx: &mut Tx) -> bool {
    if !host.new_worktree().picker.typeahead.pending() {
        return false;
    }
    let (before, effects, after) = {
        let ta = &mut host.new_worktree_mut().picker.typeahead;
        let before = ta.phase.clone();
        let effects = ta.step(typeahead::Event::Tick { now });
        (before, effects, ta.phase.clone())
    };
    branch_effects(host, effects, tx);
    before != after
}

fn branch_effects(host: &dyn NewWorktreeHost, effects: rune::machine::Effects<typeahead::Effect>, tx: &mut Tx) {
    for effect in effects {
        match effect {
            typeahead::Effect::WakeAt(due) => tx.wake_at(due),
            typeahead::Effect::Request(query) => ask_branches(host, query),
        }
    }
}

/// Asks the server for the checkout's branches matching `search`.
fn ask_branches(host: &dyn NewWorktreeHost, search: String) {
    host.send_request(Request::Branches { directory: host.new_worktree().picker.directory.clone(), search });
}

fn branch_rows(host: &dyn NewWorktreeHost) -> Vec<String> {
    let picker = &host.new_worktree().picker;
    let mut rows = picker.rows().0;
    if picker.search.text.is_empty() {
        rows.insert(0, CHECKOUT.to_owned());
    }
    rows
}

fn active_branch(host: &dyn NewWorktreeHost, rows: &[String]) -> Option<String> {
    host.new_worktree().picker.active.resolve(rows.iter().map(String::as_str))
}

fn choose_branch(host: &mut dyn NewWorktreeHost, branch: String, tx: &mut Tx) {
    if branch == CHECKOUT {
        if let Some(dir) = host.draft_project().map(|p| p.directory.clone()) {
            host.set_draft_dir(dir);
        }
    } else if let Some(id) = host.draft_id().map(str::to_owned) {
        host.new_worktree_mut().drafts.insert(id, Some(branch));
    }
    close_branch_picker(host, tx);
}

fn branch_picker_key(host: &mut dyn NewWorktreeHost, ev: &KeyEvent, tx: &mut Tx) -> bool {
    let rows = branch_rows(host);
    let at = active_branch(host, &rows).and_then(|a| rows.iter().position(|r| *r == a));
    let step = |d: isize| at.map_or(0, |i| (i as isize + d).rem_euclid(rows.len().max(1) as isize) as usize);
    match ev.key {
        Key::Escape => close_branch_picker(host, tx),
        Key::Down | Key::Up if !rows.is_empty() => {
            tx.instant();
            let i = step(if ev.key == Key::Down { 1 } else { -1 });
            tx.scroll_into_view_in("branch-picker-scroll", format!("branch-{}", rows[i]));
            host.new_worktree_mut().picker.active.set(rows[i].clone());
        }
        Key::Enter => {
            // The user's choice never rests on a row the pending query hasn't confirmed.
            if let Some(i) = at.filter(|&i| rows[i] == CHECKOUT || !host.new_worktree().picker.typeahead.stale) {
                choose_branch(host, rows[i].clone(), tx);
            }
        }
        _ => return false,
    }
    true
}

fn branch_search_changed(host: &mut dyn NewWorktreeHost, tx: &mut Tx) {
    tx.instant();
    let effects = {
        let picker = &mut host.new_worktree_mut().picker;
        let input = picker.search.text.clone();
        let effects = picker.typeahead.step(typeahead::Event::Input { input: &input, now: tx.clock() });
        let keys: Vec<&str> = picker.typeahead.shown.iter().map(String::as_str).collect();
        picker.active.step(HighlightEvent::Rows(&keys));
        effects
    };
    branch_effects(host, effects, tx);
}

fn toggle_branch_picker(host: &mut dyn NewWorktreeHost, tx: &mut Tx) {
    if host.new_worktree().picker.is_open() {
        close_branch_picker(host, tx);
    } else {
        open_branch_picker(host, tx);
    }
}

fn clear_branch_search(host: &mut dyn NewWorktreeHost, tx: &mut Tx) {
    let picker = &mut host.new_worktree_mut().picker;
    picker.search = TextEdit::new("");
    picker.search.focused = true;
    branch_search_changed(host, tx);
}

fn hover_branch(host: &mut dyn NewWorktreeHost, branch: &str, inside: bool, tx: &mut Tx) {
    if inside {
        tx.instant();
        host.new_worktree_mut().picker.active.step(HighlightEvent::Pointer(branch.to_owned()));
    }
}

fn close_branch_picker_outside(host: &mut dyn NewWorktreeHost, tx: &mut Tx) {
    close_branch_picker_for(host, Close::Outside, tx);
}

fn trigger_shell(host: &dyn NewWorktreeHost, p: &Pal, current: &str) -> El {
    let open = host.new_worktree().picker.is_open();
    let branch = draft_base(host).unwrap_or(current);
    let el = row()
        .key(TRIGGER).role("button").label_static(opencode_i18n::t!("rune.branch.choose")).expanded(open)
        .focus_inset(p)
        .hover_group().h(28.0).max_w(220.0).shrink().min_w(0.0).px(6.0).gap(6.0).center_y().radius(4.0)
        .fill(if open { p.pressed } else { Color::TRANSPARENT })
        .hover(if open { Color::TRANSPARENT } else { p.hover })
        .child(icon(&ic::BRANCH_OUT, 16.0).color(p.icon_muted))
        .child(text(opencode_i18n::t!("session.new.workspace.fromBranch", branch = branch)).font(UI).color(if open { p.text } else { p.muted }).group_color(p.text).truncate_fade())
        .child(icon(&ic::CHEVRON_DOWN, 14.0).ml(-2.0).color(p.icon_muted));
    if open {
        return el;
    }
    // Two commands, each with its shortcut: Choose worktree, then Choose branch (official
    // `new-session/workspace/selector.tsx` `BaseBranchPicker`, opencode#66097).
    el.help(opencode_i18n::t!("session.location.worktree")).help_keys(Bind::LocationSelect.keycaps())
        .help_row(opencode_i18n::t!("command.session.branch.choose")).help_keys(Bind::BranchChoose.keycaps())
}

/// The branch beside a new worktree (`selector.tsx:159-176`): `from <branch>` after the
/// branch-out glyph and before a small chevron, muted until the pointer is over it or its
/// picker is open, its 4 pt corners lit with the hover overlay (the pressed one while open).
pub fn trigger<H: NewWorktreeHost + 'static>(s: &H, p: &Pal, current: &str) -> El {
    trigger_shell(s, p, current).on_press_tx(|s: &mut H, tx: &mut Tx| toggle_branch_picker(s, tx))
}

fn picker_input_shell(edit: &TextEdit, p: &Pal) -> El {
    opencode_ui::style::field(edit, UI)
        .key("branch-picker-input").single_line().grow(1.0).shrink().min_w(0.0).color(p.text)
        .placeholder(opencode_i18n::t!("session.new.workspace.branch.search.placeholder"), p.faint)
        .caret(p.text, p.selection).selection_style(p.field_selection)
}

fn picker_clear_shell(p: &Pal) -> El {
    el().key("branch-picker-clear").role("button").label_static(opencode_i18n::t!("ui.list.clearFilter"))
        .size(20.0, 20.0).radius(4.0).center().focus_inset(p).hover(p.hover)
        .child(icon(&ic::CIRCLE_XMARK, 14.0).color(p.icon_faint))
}

fn picker_row_shell(p: &Pal, b: &str, active: Option<&str>, stale: bool, selected: Option<&str>, checkout_branch: &str) -> El {
    let checkout = b == CHECKOUT;
    let title = if checkout { opencode_i18n::t!("session.new.workspace.useMainCheckout") } else { b };
    row()
        .key(format!("branch-{b}")).role("option").label(title.to_owned())
        .w_full().h(28.0).px(8.0).gap(8.0).center_y().radius(4.0)
        .fill(if active == Some(b) { p.hover } else { Color::TRANSPARENT })
        // Rows the pending query hasn't confirmed (`opacity-50`).
        .opacity(if stale { 0.5 } else { 1.0 })
        .child(if checkout { text(title.to_owned()).font(UI).color(p.text).truncate() }
            else { text(title.to_owned()).font(UI).color(p.text).shrink().min_w(0.0).truncate() })
        .children(checkout.then(|| text(checkout_branch.to_owned()).font(UI).color(p.muted).shrink().min_w(0.0).truncate()))
        .child(rune::spacer())
        .children((selected == Some(b)).then(|| icon(&ic::CHECK, 14.0).color(p.icon_muted)))
}

/// The open picker under its trigger, and the layer that closes it.
pub fn picker<H: NewWorktreeHost + 'static>(s: &H, p: &Pal) -> Option<[El; 2]> {
    let host: &dyn NewWorktreeHost = s;
    if !host.new_worktree().picker.is_open() || host.draft_id().is_none() {
        return None;
    }
    let picker = &host.new_worktree().picker;
    let rows = branch_rows(host);
    let stale = picker.typeahead.stale;
    let active = active_branch(host, &rows);
    let checkout_branch = host.checkout_branch(host.draft_dir()).unwrap_or("");
    let selected = draft_base(host).map(str::to_owned).or_else(|| host.checkout_branch(host.draft_dir()).map(str::to_owned));
    // The search row: 32 tall, 12 in and 8 from the end, the glyph 8 before the field.
    let search = row()
        .key("branch-picker-search").h(32.0).pl(12.0).pr(8.0).gap(8.0).center_y()
        .child(icon(&ic::MAGNIFYING_GLASS, 14.0).color(p.icon_muted))
        .child(
            picker_input_shell(&picker.search, p)
                .on_edit(|s: &mut H| &mut s.new_worktree_mut().picker.search)
                .on_input_change(|s: &mut H, tx: &mut Tx| branch_search_changed(s, tx))
                .on_input_key(|s: &mut H, ev, tx| branch_picker_key(s, ev, tx)),
        )
        .children((!picker.search.text.is_empty()).then(|| picker_clear_shell(p).on_press_tx(|s: &mut H, tx: &mut Tx| clear_branch_search(s, tx))));
    let mut list = col().key("branch-picker-rows").w_full().padding(4.0);
    let list_h = 8.0 + if rows.is_empty() { 36.0 } else { 28.0 * rows.len() as f32 };
    if rows.is_empty() {
        let words = if picker.typeahead.failed() { opencode_i18n::t!("rune.branch.loadFailed") } else { opencode_i18n::t!("palette.empty") };
        list = list.child(row().key("branch-picker-empty").h(36.0).mx(4.0).px(8.0).py(4.0).center_y().child(text(words).font(UI).color(p.faint)));
    }
    for b in &rows {
        let (pick, hover) = (b.clone(), b.clone());
        list = list.child(
            picker_row_shell(p, b, active.as_deref(), stale, selected.as_deref(), checkout_branch)
                .on_hover_change(move |s: &mut H, inside, tx: &mut Tx| hover_branch(s, &hover, inside, tx))
                .on_press_tx(move |s: &mut H, tx: &mut Tx| choose_branch(s, pick.clone(), tx)),
        );
    }
    // `w-[243px] rounded-lg bg-layer-01`, `--v2-elevation-floating`, 4 pt under the trigger.
    let surface = col()
        .key("branch-picker").role("dialog").label_static(opencode_i18n::t!("rune.branch.choose"))
        .w(243.0).radius(8.0).fill(p.layer1).elevate(p.floating).clip_soft(0.0)
        .transition(Transition::none()).motion(rune::Spring::NONE)
        .on_press(|_: &mut H| {})
        .child(search)
        .child(el().key("branch-picker-divider").h(0.5).w_full().fill(p.border))
        // The location dialog's `MenuScroll` (`new-session/location/dialog.tsx:335`).
        .child(opencode_ui::scroller::scroller("branch-picker-scroll").bar(opencode_ui::scroller::Bar::MenuScroll).build::<H>(p, col().w_full().h(list_h.min(224.0)), list));
    let scrim = el()
        .key("branch-picker-scrim").absolute(0.0, 0.0).w_full().h_full().transition(Transition::none())
        .on_background_press_tx(|s: &mut H, tx: &mut Tx| close_branch_picker_outside(s, tx));
    Some([scrim, surface.anchored_fit(TRIGGER.to_owned(), Side::Bottom, Align::Start, 4.0)])
}
