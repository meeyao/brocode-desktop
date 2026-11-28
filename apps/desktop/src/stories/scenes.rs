use opencode_kit::model::Message;

use crate::{App, Route};

pub(crate) fn open_scene(s: &mut App) {
    let id = s.fixture_run.fixture["open"].as_str().map(str::to_owned).or_else(|| s.sessions.first().map(|x| x.id.clone()));
    if let Some(id) = id {
        *s.route = Route::here(id.clone());
    }
}

pub(crate) fn busy_scene(s: &mut App) {
    open_scene(s);
    s.finish_stream();
    if let Route::Session { id, .. } = (*s.route).clone() {
        if let Some(Message::Assistant(a)) = s.transcripts.get_mut(&id).and_then(|t| t.messages.last_mut()) {
            a.completed = None;
        }
        s.running.insert(id);
    }
}

/// Shows the scene in side tabs rather than Desktop's default top tabs.
pub(crate) fn side_tabs(s: &mut App) {
    s.settings.tab_layout = crate::layout_state::TabLayout::Vertical;
    *s.layout = crate::layout_state::Shell::at_rest(s.settings.tab_layout);
}

/// A named scene: `--snap SCENE` and `--list-scenes`.
pub(crate) type Scene = (&'static str, fn(&mut App));

/// The app's scenes in the order `--list-scenes` shows them. One not named here goes after these, by
/// module path.
const ORDER: &[&str] = &[
    "home",
    "no-server",
    "session",
    "streaming",
    "busy",
    "busy-deliver",
    "queue",
    "queue-3",
    "queue-5",
    "queue-25",
    "queue-50",
    "queue-question",
    "queue-question-3",
    "queue-question-morph",
    "draft",
    "settings",
    "settings-sidebar-closed",
    "question",
    "question-composer",
    "question-morph",
    "question-long",
    "sidebar-closed",
    "layout-side-tabs",
    "tab-overflow",
    "status",
    "tab-badges",
    "demo",
];

pub(crate) fn all() -> Vec<Scene> {
    crate::stories::ordered(rune_register::collect(&SCENE_LIST), ORDER, |s| s.0)
}

rune_register::slice!(pub(crate) SCENE_LIST: Scene);

rune_register::register!(SCENE_LIST: Scene = [
    ("home", |_| {}),
    ("session", |s| {
        open_scene(s);
        s.finish_stream();
    }),
    ("streaming", |s| {
        open_scene(s);
        // Playback starts on the first poll.
        if let Some(id) = s.fixture_run.fixture["open"].as_str().map(str::to_owned) {
            s.start_stream_if_due(&id);
        }
    }),
    // The open session's turn still running, so Enter steers and ⌘⏎ queues.
    ("busy", busy_scene),
    // Running turn with two active subagents so Working sits above the floating subagents pill.
    ("busy-subagents", |s| {
        busy_scene(s);
        let parent = crate::stories::sid(s);
        let dir = (*s.default_dir).clone();
        for (k, title) in [(1, "Audit timeline motion"), (2, "Check scroll anchoring")] {
            let id = format!("ses_busy_sub_{k}");
            s.sessions.push(opencode_kit::model::SessionInfo {
                id: id.clone(),
                title: title.into(),
                directory: dir.clone(),
                parent: Some(parent.clone()),
                updated: *s.now_ms - (k as i64) * 1000,
                ..Default::default()
            });
            s.running.insert(id);
        }
    }),
    // The open session's own turn over and two subagents still running under it: its status
    // line says it waits for them, over their capsule.
    ("waiting-subagents", |s| {
        open_scene(s);
        s.finish_stream();
        let parent = crate::stories::sid(s);
        let dir = (*s.default_dir).clone();
        for (k, title) in [(1, "Audit timeline motion"), (2, "Check scroll anchoring")] {
            let id = format!("ses_waiting_sub_{k}");
            s.sessions.push(opencode_kit::model::SessionInfo { id: id.clone(), title: title.into(), directory: dir.clone(), parent: Some(parent.clone()), updated: *s.now_ms - (k as i64) * 1000, ..Default::default() });
            s.running.insert(id);
        }
    }),
    // Busy, then the server takes the steer at 5.6 s and the queued prompts at 6.6 and
    // 7.6 s: with `apps/desktop/scripts/steer-dock.json`, a steer and two queued prompts delivered.
    ("busy-deliver", |s| {
        busy_scene(s);
        let Route::Session { id, .. } = (*s.route).clone() else { return };
        let next = |kind: &str| serde_json::json!({ "sessionID": id, "next": kind });
        *s.fixture_run.playback = [(5.6, "session.inbox.delivered".into(), next("steer")), (6.6, "session.inbox.delivered".into(), next("queue")), (7.6, "session.inbox.delivered".into(), next("queue"))].into();
    }),
    ("draft", |s| {
        s.open_draft_tab();
    }),
    ("settings", |s| {
        open_scene(s);
        s.finish_stream();
        s.open_settings();
    }),
    // The storybook (⌘⇧D)'s stories are scenes too, under their paths ("Timeline/Send"):
    // `--playground PATH [--variant NAME]` opens one with its motion playing (`stories/mod.rs`).
    ("sidebar-closed", |s| {
        open_scene(s);
        s.finish_stream();
        side_tabs(s);
        *s.sidebar_open = false;
    }),
    // Settings › Interface layout: Side tabs, with the session open.
    ("layout-side-tabs", |s| {
        open_scene(s);
        s.finish_stream();
        side_tabs(s);
    }),
    // Consolidated List: pinned area + grouped/sorted Recent in the sidebar (forces vertical).
    ("sidebar-consolidated-list", |s| {
        open_scene(s);
        s.finish_stream();
        s.settings.sidebar_organize = crate::settings::Organize::Consolidated;
        s.settings.navigation = crate::settings::Navigation::List;
        *s.layout = crate::layout_state::Shell::at_rest(s.settings.effective_tab_layout());
    }),
    // Consolidated List grouped by status with metadata on.
    ("sidebar-consolidated-list-grouped", |s| {
        open_scene(s);
        s.finish_stream();
        s.settings.sidebar_organize = crate::settings::Organize::Consolidated;
        s.settings.navigation = crate::settings::Navigation::List;
        *s.layout = crate::layout_state::Shell::at_rest(s.settings.effective_tab_layout());
        crate::view::sidebar::recent_view::update(s, |v| {
            v.group_by = crate::view::sidebar::recent_view::GroupBy::Status;
            v.metadata = true;
        });
    }),
    // Consolidated Home with compact project scope picker and ProviderTip.
    ("home-consolidated", |s| {
        s.settings.sidebar_organize = crate::settings::Organize::Consolidated;
        s.settings.navigation = crate::settings::Navigation::Tabs;
        s.settings.tab_layout = crate::layout_state::TabLayout::Vertical;
        *s.layout = crate::layout_state::Shell::at_rest(s.settings.effective_tab_layout());
        *s.route = crate::Route::Home;
    }),
    // Settings > Experimental > Organize vertical sidebar: One list.
    ("sidebar-list", |s| {
        open_scene(s);
        s.finish_stream();
        side_tabs(s);
        s.settings.sidebar_organize = crate::settings::Organize::List;
    }),
    // Settings > Experimental > Organize vertical sidebar: Customize, grouped by project.
    ("sidebar-customized", |s| {
        open_scene(s);
        s.finish_stream();
        side_tabs(s);
        s.settings.sidebar_organize = crate::settings::Organize::Customize;
    }),
    // Customize in one list, by last update, subagents shown.
    ("sidebar-customized-list", |s| {
        open_scene(s);
        s.finish_stream();
        side_tabs(s);
        s.settings.sidebar_organize = crate::settings::Organize::Customize;
        let c = &mut s.project_state.customize;
        c.grouping = crate::view::sidebar::grouping::Grouping::None;
        c.ordering = crate::view::sidebar::grouping::Ordering::Updated;
        c.show.subagents = true;
    }),
    // Settings > Experimental > Organize vertical sidebar: Recent, two sessions pinned.
    ("sidebar-recent", |s| {
        open_scene(s);
        s.finish_stream();
        side_tabs(s);
        s.settings.sidebar_organize = crate::settings::Organize::Recent;
        let server = crate::servers::runtime::active(s).as_str().to_owned();
        let ids: Vec<String> = s.sessions.iter().filter(|i| i.parent.is_none()).skip(2).take(2).map(|i| i.id.clone()).collect();
        for id in ids {
            s.project_state.recent.pin(crate::view::sidebar::state::key(&server, &id));
        }
    }),
    // Top tabs past what the titlebar holds: they shrink to icons, then scroll.
    ("tab-overflow", |s| {
        open_scene(s);
        s.finish_stream();
        let dir = (*s.default_dir).clone();
        for n in 1..=24 {
            s.tabs.push(crate::tabs::Tab::session(format!("ses_overflow_{n:02}"), format!("Overflow tab {n}"), dir.clone()));
        }
    }),
    ("status", |s| {
        open_scene(s);
        s.finish_stream();
        for e in s.fixture_run.fixture["status_events"].as_array().cloned().unwrap_or_default() {
            s.fixture_run.playback.push_back((e["t"].as_f64().unwrap_or(0.0), e["type"].as_str().unwrap_or("").into(), e["data"].clone()));
        }
    }),
]);
