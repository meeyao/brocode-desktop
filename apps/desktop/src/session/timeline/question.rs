//! Desktop host adapter for `opencode-feature-timeline-extras::question`
//! (`features/timeline-extras`).

pub use opencode_feature_timeline_extras::question::*;

use rune::{KeyEvent, Play, Transition, Tx};
use rune_knobs::Knobs;
use serde_json::Value;

use crate::stories::{Trigger, emit, replay};
use crate::{App, knobs};

pub(crate) static QUESTION: opencode_kernel::Slot<Dock> = opencode_kernel::Slot::new();

impl QuestionHost for App {
    fn question_dock(&self) -> &Dock {
        self.kernel.slots.get(&QUESTION)
    }

    fn question_dock_mut(&mut self) -> &mut Dock {
        self.kernel.slots.get_mut(&QUESTION)
    }

    fn knobs(&self) -> &Knobs {
        &self.diag.knobs
    }

    fn question_shown(&self) -> bool {
        session_view::dock::question::asking(self).is_some()
    }

    fn question_id(&self) -> Option<String> {
        session_view::dock::question::id(self)
    }

    fn question_takes_text(&self) -> bool {
        session_view::dock::question::takes_text(self)
    }

    fn question_type(&mut self, text: &str) {
        session_view::dock::question::type_text(self, text);
    }

    fn question_last(&mut self) {
        session_view::dock::question::go_last(self);
    }

    fn question_leave_field(&mut self) {
        session_view::dock::question::leave_field(self);
    }

    fn dock_morph(&mut self, tx: &mut Tx) {
        session_view::dock::morph(self, tx);
    }

    fn question_key(&mut self, ev: &KeyEvent, tx: &mut Tx) -> bool {
        session_view::dock::question::key(self, ev, tx)
    }

    fn fixture_question(&self) -> Option<&Value> {
        Some(&self.fixture_run.fixture["question"])
    }
}

impl App {
    pub fn question_shown(&self) -> bool {
        opencode_feature_timeline_extras::question::shown(self)
    }

    pub fn question_id(&self) -> Option<String> {
        opencode_feature_timeline_extras::question::id(self)
    }

    pub fn question_takes_text(&self) -> bool {
        opencode_feature_timeline_extras::question::takes_text(self)
    }

    pub fn question_type(&mut self, text: &str) {
        opencode_feature_timeline_extras::question::type_text(self, text);
    }

    pub fn question_last(&mut self) {
        opencode_feature_timeline_extras::question::go_last(self);
    }

    pub fn question_leave_field(&mut self) {
        opencode_feature_timeline_extras::question::leave_field(self);
    }

    pub fn morph(&mut self, tx: &mut Tx) {
        opencode_feature_timeline_extras::question::morph(self, tx);
    }

    pub fn question_key(&mut self, ev: &KeyEvent, tx: &mut Tx) -> bool {
        opencode_feature_timeline_extras::question::key(self, ev, tx)
    }
}

pub fn face_transition(s: &App) -> Transition {
    opencode_feature_timeline_extras::question::face_transition(s)
}

rune_register::register!(knobs::GROUP_LIST: rune_knobs::Group = [knobs::QUESTION_DOCK]);

rune_register::register!(crate::scenes::SCENE_LIST: crate::scenes::Scene = [
    ("question", |s| {
        crate::open_scene(s);
        s.finish_stream();
        let q = s.fixture_run.fixture["question"].clone();
        s.apply_event("form.created", &q);
    }),
    ("question-composer", |s| {
        crate::open_scene(s);
        s.finish_stream();
        s.settings.question_composer = Some(true);
        let q = s.fixture_run.fixture["question"].clone();
        s.apply_event("form.created", &q);
    }),
    // Answered elsewhere at 1.5 s, asked again at 2.5 s: the morph both ways.
    ("question-morph", |s| {
        crate::open_scene(s);
        s.finish_stream();
        let q = s.fixture_run.fixture["question"].clone();
        s.apply_event("form.created", &q);
        let replied = scene_replied_payload(&q);
        *s.fixture_run.playback = [(1.5, "form.replied".into(), replied), (2.5, "form.created".into(), q)].into();
    }),
    ("question-long", |s| {
        crate::open_scene(s);
        s.finish_stream();
        let q = long::form(s);
        s.apply_event("form.created", &q);
    }),
]);

rune_register::register!(crate::stories::STORY_LIST: crate::stories::Story = [
    crate::stories::Story::session("Question dock/Long question", LONG_KEYS).knobs(&[knobs::QUESTION_DOCK]),
]);

const LONG_KEYS: &[Trigger] = &[
    replay(long_open),
    Trigger {
        key: 'w',
        label: "Walk the answers with ↓ and ↑",
        run: |s, tx| {
            tx.clear();
            long_walk(s, tx);
        },
    },
];

fn long_open(s: &mut App, tx: &mut Play<App>) {
    s.finish_stream();
    tx.after(0.3, |s, tx| {
        let q = long::form(s);
        emit(s, "form.created", q, tx);
    });
    tx.after(1.2, long_walk);
}

fn long_walk(_: &mut App, tx: &mut Play<App>) {
    tx.after(0.0, long_down);
}

fn long_down(s: &mut App, tx: &mut Play<App>) {
    if !long::step_down(s, tx) {
        tx.after(long::WALK_BEAT_S * 2.0, long_up);
    } else {
        tx.after(long::WALK_BEAT_S, long_down);
    }
}

fn long_up(s: &mut App, tx: &mut Play<App>) {
    if long::step_up(s, tx) {
        tx.after(long::WALK_BEAT_S, long_up);
    }
}
