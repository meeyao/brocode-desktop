//! Desktop host adapter for `opencode-feature-workspace-tools::home_search`
//! (`features/workspace-tools`).

pub use opencode_feature_workspace_tools::home_search::*;

use opencode_kit::client::Server;
use opencode_kit::model::{Project, SessionInfo};
use rune::{El, Key, KeyEvent, Play, TextEdit, Tx, col};
use session_view::palette::Pal;

use crate::pickers::PickerHost as _;
use crate::stories::{Story, Trigger, replay, stage};
use crate::{App, Route, Source};

pub(crate) static HOME_SEARCH: opencode_kernel::Slot<HomeSearch> = opencode_kernel::Slot::new();

impl HomeSearchHost for App {
    fn home_search(&self) -> &HomeSearch {
        self.kernel.slots.get(&HOME_SEARCH)
    }

    fn home_search_mut(&mut self) -> &mut HomeSearch {
        self.kernel.slots.get_mut(&HOME_SEARCH)
    }

    fn search_input(&self) -> &TextEdit {
        &self.search
    }

    fn search_input_mut(&mut self) -> &mut TextEdit {
        &mut self.search
    }

    fn is_home(&self) -> bool {
        *self.route == Route::Home
    }

    fn settings_open(&self) -> bool {
        App::settings_open(self)
    }

    fn search_inline(&self) -> bool {
        self.settings.navigation_prototype()
    }

    fn live_server(&self) -> Option<Server> {
        match &*self.source {
            Source::Live { server, .. } => Some(server.clone()),
            Source::Fixture => None,
        }
    }

    fn home_search_scope(&self) -> (String, bool) {
        let selection = crate::projects::home::selection(self);
        let scope = format!(
            "{}\0{}",
            selection.server.as_ref().map_or("", |k| k.as_str()),
            selection.directory.as_deref().unwrap_or(""),
        );
        let key = crate::projects::home::server(self);
        let connected = matches!(*self.source, Source::Live { .. })
            && self.kernel.slots.get(&crate::projects::HOME_STATE).servers.iter().any(|v| v.key == key && v.healthy());
        (scope, connected)
    }

    fn home_selected(&self) -> Option<&Project> {
        App::home_selected(self)
    }

    fn session(&self, id: &str) -> Option<&SessionInfo> {
        App::session(self, id)
    }

    fn push_session(&mut self, session: SessionInfo) {
        self.sessions.push(session);
    }

    fn search_session(&self, id: &str) -> Option<&SessionInfo> {
        App::search_session(self, id)
    }

    fn project_for_session(&self, session: &SessionInfo) -> Option<&Project> {
        App::project_for_session(self, session)
    }

    fn session_in_project(&self, session: &SessionInfo, project: &Project) -> bool {
        let dir = opencode_kit::path::trim(session.directory.trim());
        let root = opencode_kit::path::trim(project.directory.trim());
        if (!dir.is_empty() && opencode_kit::path::same(dir, root)) || self.has_worktree_at(project, dir) {
            return true;
        }
        // Worktree inventories load on demand (`packages/app/src/home/sessions/records.ts:48-58`);
        // fall back to direct `session.project` match only for non-"global" git projects (never
        // `project_containing`, so nested repos cannot leak into a parent git project).
        project.id != "global" && !session.project.is_empty() && session.project == project.id
    }

    fn project_directories(&self, project: &Project) -> Vec<String> {
        let mut dirs = vec![project.directory.clone()];
        dirs.extend(self.project_worktrees(&project.id, &project.directory));
        dirs
    }

    fn merged_roots(&self) -> Vec<&SessionInfo> {
        App::merged_roots(self, &[])
    }

    fn merged_roots_with<'a>(&'a self, extra: &'a [SessionInfo]) -> Vec<&'a SessionInfo> {
        App::merged_roots(self, extra)
    }

    fn find_home_sessions(&self, project_id: Option<&str>, query: &str, limit: usize) -> Vec<(i32, i64, Box<str>)> {
        let scope = crate::switcher::search_index::Scope::Home {
            project: project_id.map(str::to_owned),
        };
        self.find_sessions(scope, query, limit)
    }

    fn session_search_building(&self) -> bool {
        App::session_search_building(self)
    }

    fn fulltext_home_rows(&self) -> &[crate::fulltext::Row] {
        self.fulltext_rows(crate::fulltext::Surface::Home)
    }

    fn before_focus_home_search(&mut self) {
        self.close_picker(false);
        self.project_state.home_menu_open = false;
    }

    fn prefetch_session(&self, id: &str) {
        App::prefetch_session(self, id);
    }

    fn open_session(&mut self, id: &str, tx: &mut Tx) {
        self.open(id, tx);
    }

    fn open_find_at(&mut self, query: &str, message: &str, tx: &mut Tx) {
        App::open_find_at(self, query, message, tx);
    }

    fn handle_fallback_key(&mut self, ev: &KeyEvent, tx: &mut Tx) -> bool {
        self.switcher_key(ev, tx) || self.shortcut(ev, tx)
    }
}

impl App {
    pub fn focus_home_search(&mut self, tx: &mut Tx) {
        opencode_feature_workspace_tools::home_search::focus_home_search(self, tx);
    }

    pub fn clear_home_search(&mut self, keep_focus: bool, tx: &mut Tx) {
        opencode_feature_workspace_tools::home_search::clear_home_search(self, keep_focus, tx);
    }

    pub fn home_sessions<'a>(&'a self, selected: Option<&Project>) -> Vec<&'a SessionInfo> {
        opencode_feature_workspace_tools::home_search::home_sessions(self, selected)
    }

    pub fn home_search_results<'a>(&'a self, selected: Option<&Project>) -> Vec<&'a SessionInfo> {
        opencode_feature_workspace_tools::home_search::home_search_results(self, selected)
    }

    pub(crate) fn home_roots<'a>(&'a self, selected: Option<&Project>) -> impl Iterator<Item = &'a SessionInfo> {
        opencode_feature_workspace_tools::home_search::home_roots(self, selected)
    }

    pub fn open_home_result(&mut self, id: &str, tx: &mut Tx) {
        opencode_feature_workspace_tools::home_search::open_home_result(self, id, tx);
    }

    pub fn home_search_open(&self) -> bool {
        opencode_feature_workspace_tools::home_search::home_search_open(self)
    }

    pub fn home_search_loading(&self) -> bool {
        opencode_feature_workspace_tools::home_search::home_search_loading(self)
    }

    pub fn home_search_active(&self, ids: &[String]) -> Option<String> {
        opencode_feature_workspace_tools::home_search::home_search_active(self, ids)
    }

    pub fn home_search_key(&mut self, ev: &KeyEvent, tx: &mut Tx) -> bool {
        opencode_feature_workspace_tools::home_search::home_search_key(self, ev, tx)
    }

    pub fn poll_home_search(&mut self, tx: &mut Tx) -> bool {
        opencode_feature_workspace_tools::home_search::poll_home_search(self, tx)
    }
}

/// Storybook story: `Home/Session search`.
fn home_story_view(s: &App, p: &Pal) -> El {
    col()
        .key("story-home")
        .w_full()
        .max_w(920.0)
        .h_full()
        .max_h(680.0)
        .radius(10.0)
        .fill(p.bg_base)
        .box_shadow(Pal::ring_out(p.ring))
        .shadow(1.0, 2.0, p.shadow)
        .clip_soft(0.0)
        .child(crate::view::home(s, p, [920.0, 0.0], [0.0, 0.0]))
}

const STORY_KEYS: &[Trigger] = &[
    replay(story_open),
    Trigger {
        key: 'n',
        label: "Next result",
        run: |s, tx| {
            let _ = s.home_search_key(&KeyEvent { key: Key::Down, mods: Default::default() }, tx);
        },
    },
    Trigger { key: 'c', label: "Clear or fill query", run: story_toggle },
];

const STORY_VARIANTS: &[(&str, usize)] = &[("By title", 0), ("By project", 1), ("No results", 2)];

fn story_open(s: &mut App, tx: &mut Play<App>) {
    *s.route = Route::Home;
    let q = match *s.fixture_run.variant {
        1 => "harbor",
        2 => "notpresent",
        _ => "reconnect",
    };
    *s.search = TextEdit::new(q);
    s.search.focused = true;
    s.composer.focused = false;
    tx.instant();
}

fn story_toggle(s: &mut App, tx: &mut Play<App>) {
    if s.search.text.is_empty() {
        *s.search = TextEdit::new("reconnect");
        s.search.focused = true;
        tx.instant();
    } else {
        s.clear_home_search(false, tx);
    }
}

rune_register::register!(crate::stories::STORY_LIST: Story = [
    Story { path: "Home/Session search", scene: |s| stage(s, home_story_view), knobs: &[], keys: STORY_KEYS, variants: STORY_VARIANTS },
]);
