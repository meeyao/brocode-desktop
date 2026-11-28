//! Shell › Whisper sections: the sidebar's tabs in Experimental › Sidebar sections, made up
//! (the fixture's Lumen, Harbor and Tidepool projects): one list, a tab in no section at its top,
//! the Lumen section, a hairline, then more tabs in none. Variants: that list; two tabs selected
//! with the bar showing; a new section being named (one plain field, on the name its tabs share);
//! a folded section whose tab works; and a walkthrough that plays it all through: two tabs apart
//! picked, grouped where the first was (they glide together under a new header), named, carried
//! up above the Lumen section, folded, opened and removed (its tabs stay where they are). R
//! replays; S selects two tabs, G groups them, F folds the first section.

use super::*;
use crate::stories::{Story, Trigger, replay, stage};
use crate::tabs::sections::{self as sec, Section, drag, naming, select};

const VARIANTS: &[(&str, usize)] = &[("Default", 0), ("Multi-selected", 1), ("Naming new section", 2), ("Collapsed section", 3), ("Walkthrough", 4)];

const KEYS: &[Trigger] = &[
    replay(open),
    Trigger { key: 's', label: "Select two tabs", run: |s, _| pick(s) },
    Trigger { key: 'g', label: "Group the selection", run: |s, tx| {
        let uids = sec::state(s).selection.uids().to_vec();
        sec::create(s, &uids, tx);
    } },
    Trigger { key: 'f', label: "Fold the first section", run: |s, tx| {
        if let Some(id) = sec::state(s).doc.sections.first().map(|x| x.id.clone()) {
            sec::toggle(s, &id, tx);
        }
    } },
];

const TABS: [(&str, &str); 8] = [
    ("ses_fx0011aaaaaaaaaaaaaaaaaa", "HOME"),
    ("ses_fx0001aaaaaaaaaaaaaaaaaa", "LUMEN TOKENIZER STATS"),
    ("ses_fx0003aaaaaaaaaaaaaaaaaa", "LUMEN ROUTING RULES"),
    ("ses_fx0006aaaaaaaaaaaaaaaaaa", "Deploy preview builds"),
    ("ses_fx0007aaaaaaaaaaaaaaaaaa", "Deploy cold starts"),
    ("ses_fx0008aaaaaaaaaaaaaaaaaa", "Sync engine rewrite"),
    ("ses_fx0009aaaaaaaaaaaaaaaaaa", "Offline cache"),
    ("ses_fx0010aaaaaaaaaaaaaaaaaa", "Release notes draft"),
];

fn uid(s: &App, i: usize) -> String {
    s.tabs.get(i).map(crate::tabs::Tab::uid).unwrap_or_default()
}

fn setup(s: &mut App) {
    crate::open_scene(s);
    s.finish_stream();
    crate::scenes::side_tabs(s);
    s.settings.sidebar_organize = crate::settings::Organize::List;
    s.settings.tab_groups = Some(false);
    s.settings.sidebar_sections = Some(true);
    let mut tabs = Vec::new();
    for (id, title) in TABS {
        if let Some(info) = s.sessions.iter_mut().find(|x| x.id == id) {
            info.title = title.to_owned();
            tabs.push(crate::tabs::Tab::session(id, title, info.directory.clone()));
        }
    }
    *s.tabs = tabs;
    if let Some(t) = s.tabs.first() {
        *s.route = Route::here(t.id.clone());
    }
    let lumen = vec![uid(s, 1), uid(s, 2)];
    let st = sec::state_mut(s);
    st.doc.sections = vec![Section { id: "story-lumen".into(), name: "Lumen".into(), collapsed: false, members: lumen }];
    st.selection = select::Selection::Empty;
    st.naming = Default::default();
    st.drag = Default::default();
}

fn pick(s: &mut App) {
    let (a, b) = (uid(s, 3), uid(s, 4));
    let st = sec::state_mut(s);
    st.selection.step(select::Event::Toggle(&a));
    st.selection.step(select::Event::Toggle(&b));
}

fn open(s: &mut App, play: &mut rune::Play<'_, App>) {
    setup(s);
    match *s.fixture_run.variant {
        1 => pick(s),
        2 => {
            pick(s);
            let uids = sec::state(s).selection.uids().to_vec();
            sec::create(s, &uids, play);
        }
        3 => {
            if let Some(first) = sec::state_mut(s).doc.sections.first_mut() {
                first.collapsed = true;
            }
            let working = s.tabs.get(2).map(|t| t.id.clone());
            if let Some(id) = working {
                s.running.insert(id);
            }
            let open = s.tabs.get(5).map(|t| t.id.clone());
            if let Some(id) = open {
                *s.route = Route::here(id);
            }
        }
        _ => {}
    }
    play.instant();
    if *s.fixture_run.variant == 4 {
        let cold = s.tabs.remove(4);
        s.tabs.insert(6, cold);
        walkthrough(play);
    }
}

fn walkthrough(play: &mut rune::Play<'_, App>) {
    play.after(0.55, |s, play| pick_one(s, play, 3));
    play.after(1.15, |s, play| pick_one(s, play, 6));
    play.after(1.85, |s, play| {
        play.animated();
        let uids = sec::state(s).selection.uids().to_vec();
        sec::create(s, &uids, play);
    });
    play.after(3.15, |s, play| {
        sec::naming_step(s, naming::Event::Commit, play);
        play.blur();
        play.animated();
    });
    play.after(3.9, |s, play| carry(s, play, None));
    for (k, dy) in [-12.0, -30.0, -52.0, -76.0, -98.0].into_iter().enumerate() {
        play.after(4.0 + k as f64 * 0.12, move |s, play| carry(s, play, Some(dy)));
    }
    play.after(4.85, |s, play| {
        if let drag::Effect::Reorder { id, to } = drag::step(&mut sec::state_mut(s).drag, drag::Event::Up { on_self: true }) {
            sec::reorder_section(s, &id, to);
        }
        drag::step(&mut sec::state_mut(s).drag, drag::Event::Press);
        play.animated();
    });
    play.after(5.8, fold_new);
    play.after(6.8, fold_new);
    play.after(7.8, |s, play| {
        if let Some(id) = newest(s) {
            sec::remove_section(s, &id, play);
        }
        play.animated();
    });
}

fn carry(s: &mut App, play: &mut rune::Play<'_, App>, dy: Option<f32>) {
    let Some(id) = newest(s) else { return };
    let event = match dy {
        None => {
            let uids = sec::state(s).doc.get(&id).map(|x| x.members.clone()).unwrap_or_default();
            drag::Event::DownSection { id, uids, items: sec::shown_items(s), start_scroll: 0.0, touch: false }
        }
        Some(dy) => drag::Event::MoveSection { dy, scroll_y: 0.0 },
    };
    drag::step(&mut sec::state_mut(s).drag, event);
    play.animated();
}

fn pick_one(s: &mut App, play: &mut rune::Play<'_, App>, i: usize) {
    play.animated();
    let u = uid(s, i);
    sec::state_mut(s).selection.step(select::Event::Toggle(&u));
}

fn newest(s: &App) -> Option<String> {
    sec::state(s).doc.sections.iter().find(|x| x.id != "story-lumen").map(|x| x.id.clone())
}

fn fold_new(s: &mut App, play: &mut rune::Play<'_, App>) {
    if let Some(id) = newest(s) {
        sec::toggle(s, &id, play);
    }
}

fn view(s: &App, p: &Pal) -> El {
    col()
        .key("story-sections-sidebar")
        .w(*s.sidebar_w)
        .h(440.0)
        .pt(12.0)
        .pl(INSET_START)
        .pr(INSET_END)
        .pb(10.0)
        .radius(10.0)
        .fill(p.bg_deep)
        .box_shadow(Pal::ring_out(p.ring))
        .clip_soft(0.0)
        .child(super::list(s, p, None))
}

rune_register::register!(crate::stories::STORY_LIST: Story = [
    Story { path: "Shell/Whisper sections", scene: |s| { setup(s); stage(s, view); }, knobs: &[], keys: KEYS, variants: VARIANTS },
]);
