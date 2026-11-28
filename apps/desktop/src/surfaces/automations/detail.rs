//! The inspector beside the list: the selected automation's name, status and actions, then when
//! it runs, what it does and how its last run went, then where it lives
//! (packages/app/src/automations/view.tsx:AutomationInspector).

use opencode_i18n::t;
use opencode_kit::model::{AutomationAction, AutomationOutcome, AutomationStatus, AutomationTarget, AutomationTrigger};
use opencode_ui::palette::{Pal, kit};
use opencode_ui::widgets::button;
use rune::{Direction, El, Tx, col, el, row, text};
use rune_kit::ButtonVariant;
use rune_theme_opencode::icons as ic;
use serde_json::Value;

use crate::App;

use super::AUTOMATIONS;
use super::format::{Delivery, described_cron, format_date, outcome_note, relative_time, split_output, status_label};
use super::parts::{BODY, BODY_MED, CODE, META, META_MED, TITLE, body_card, code_inline, field, icon_button, session_link, status_dot};
use super::source::{self, Entry, MutatePhase, Operation};

/// The card that holds what a run does: its prompt, code or command.
pub(super) const DO_BODY: &str = "auto-do-body";

pub(super) fn inspector(s: &App, p: &Pal, entry: &Entry, context: &[String]) -> El {
    let info = &entry.row;
    let primary = col()
        .key("auto-detail-primary")
        .w_full()
        .gap(20.0)
        .child(field(p, t!("automations.when"), BODY, when_block(s, p, entry)))
        .child(field(p, t!("automations.do"), BODY, do_block(s, p, entry)))
        .child(field(p, t!("automations.lastRun"), BODY, last_run_block(s, p, entry)));

    let mut meta = col().key("auto-detail-meta").w_full().gap(4.0);
    let folder = s.tilde(&entry.server, &info.directory);
    meta = meta.child(field(
        p,
        t!("automations.folder"),
        META,
        // A path's end names the folder: the cut falls at its start, the whole path on hover.
        text(folder).key("auto-detail-folder").font(CODE).color(p.muted).dir(Direction::Ltr).truncate_head().help(info.directory.clone()),
    ));
    // Only a creator the app can name: a raw session ID says nothing (view.tsx:AutomationInspector).
    if let (Some(id), Some(name)) = (&info.session_id, &entry.creator) {
        meta = meta.child(field(
            p,
            t!("automations.creator"),
            META,
            row().child(session_link(p, "auto-creator-link", entry.server.clone(), id, name.clone(), META_MED)),
        ));
    }

    let article = col()
        .key("auto-detail-article")
        .role("article")
        .w_full()
        .max_w(720.0)
        .px(32.0)
        .pt(24.0)
        .pb(32.0)
        .gap(24.0)
        .child(header(s, p, entry, context))
        .child(primary)
        // A hairline, as between the list's groups: a step under the pane's own borders.
        .child(el().w_full().h(1.0).fill(p.border_muted).a11y_hidden())
        .child(meta);
    // `mx-auto`: the article keeps its measure and sits in the middle of a wide pane.
    col().w_full().items_center().child(article)
}

/// The automation's name over its status, with Pause or Resume and Delete at the trailing edge.
fn header(s: &App, p: &Pal, entry: &Entry, context: &[String]) -> El {
    let info = &entry.row;
    let busy = !matches!(s.kernel.slots.get(&AUTOMATIONS).mutate, MutatePhase::Idle);

    let mut status = vec![status_label(info).to_owned()];
    status.extend(context.iter().cloned());

    let mut actions = row().key("auto-detail-actions").gap(4.0).center_y();
    if info.status != AutomationStatus::Done {
        let paused = info.status == AutomationStatus::Paused;
        let label = if paused { t!("automations.action.resume") } else { t!("automations.action.pause") };
        let op = if paused { Operation::Resume } else { Operation::Pause };
        let target = entry.clone();
        actions = actions.child(
            button(label, ButtonVariant::Neutral)
                .disabled(busy)
                .on_press_tx(move |s: &mut App, tx: &mut Tx| source::mutate(s, &target, op, tx))
                .build(kit(p))
                .key("auto-pause-resume"),
        );
    }
    // Official's Delete is a ghost-muted word beside Pause (`view.tsx:150`). It asks first
    // (`delete_dialog`), so here it is a trash glyph that stays quiet until the pointer is on it.
    // Its wash bleeds past the article's edge by its 6 pt of padding and the 1.5 pt of air in the
    // glyph's own box, so the trash's ink stands on the edge the cards and the rule keep.
    let delete = icon_button(p, "auto-delete", t!("automations.delete.title"), &ic::OUTLINE_TRASH, p.danger).mr(-7.5);
    actions = actions.child(if busy {
        delete.disabled(true).opacity(0.5)
    } else {
        let (key, server, id, name) = (entry.key.clone(), entry.server.clone(), info.id.clone(), info.name.clone());
        delete.on_press_tx(move |s: &mut App, tx: &mut Tx| {
            tx.instant();
            super::ask_delete(s, key.clone(), server.clone(), id.clone(), name.clone());
        })
    });

    row()
        .key("auto-detail-header")
        .w_full()
        .gap(16.0)
        .items_start()
        .child(
            col()
                .grow(1.0)
                .shrink()
                .min_w(0.0)
                // As tall as the buttons beside it, so a one-line name centres on them.
                .child(row().w_full().min_h(28.0).center_y().child(text(info.name.clone()).key("auto-detail-name").font(TITLE).color(p.text).wrap().dir(Direction::Auto).grow(1.0).shrink().min_w(0.0)))
                .child(
                    row()
                        .key("auto-detail-status")
                        .w_full()
                        .gap(6.0)
                        .center_y()
                        .child(status_dot(p, info))
                        .child(text(status.join(" · ")).font(BODY).color(p.muted).truncate()),
                ),
        )
        .child(actions)
}

/// The schedule in words, then its time zone and next run, with the cron text it was read from
/// kept beside them in mono. An event shows its filters instead.
fn when_block(s: &App, p: &Pal, entry: &Entry) -> El {
    let info = &entry.row;
    // A cron with no description is its own label: mono, as the text it is.
    let raw_cron = matches!(info.trigger, AutomationTrigger::Cron { .. }) && described_cron(info).is_none();
    let label = text(entry.trigger.clone()).key("auto-when-trigger").color(p.text);
    let label = if raw_cron { label.font(CODE).dir(Direction::Ltr).wrap_anywhere() } else { label.font(BODY).wrap() };
    let mut block = col().w_full().gap(2.0).child(label);

    if let AutomationTrigger::Event { args, .. } = &info.trigger {
        let Some(map) = args else { return block };
        // Flat filters read as `environment = "staging"`; anything nested keeps its JSON.
        if map.values().any(|v| v.is_object() || v.is_array()) {
            let pretty = serde_json::to_string_pretty(&Value::Object(map.clone())).unwrap_or_default();
            return block.gap(8.0).child(body_card(s, p, &entry.key, "auto-when-args", &pretty, true));
        }
        let inline = map.iter().map(|(k, v)| format!("{k} = {v}")).collect::<Vec<_>>().join(", ");
        return block.child(text(inline).key("auto-when-args-inline").font(CODE).color(p.muted).dir(Direction::Ltr).wrap_anywhere());
    }

    let cron = described_cron(info);
    let mut line = info.tz.clone();
    let mut tip = None;
    if info.status == AutomationStatus::Active
        && let Some(next) = info.next_due_at
    {
        line = format!("{line} · {}", t!("automations.nextRun", time = relative_time(next, *s.now_ms)));
        tip = Some(format_date(next, &info.tz));
    }
    let mut zone = text(line).key("auto-when-tz").font(BODY).color(p.muted);
    if let Some(tip) = tip {
        zone = zone.help(tip);
    }
    // A word space between the three, so the dot sits evenly between the words and the cron.
    let mut sub = row().w_full().flex_wrap().gap(4.0).center_y().child(zone);
    if let Some(cron) = cron {
        sub = sub
            .child(text("·").font(BODY).color(p.muted).a11y_hidden())
            .child(code_inline(text(cron.to_owned())).key("auto-when-cron").color(p.faint).dir(Direction::Ltr));
    }
    block = block.child(sub);
    block
}

/// What a run does in a sentence, then the prompt, code or command itself in its card.
fn do_block(s: &App, p: &Pal, entry: &Entry) -> El {
    let block = col().w_full().gap(8.0);
    let card = |slot: &'static str, body: &str, code: bool| body_card(s, p, &entry.key, slot, body, code);
    let summary = |line: &'static str, hint: Option<&'static str>| {
        col()
            .w_full()
            .child(text(line).font(BODY).color(p.text).wrap())
            .children(hint.map(|hint| text(hint).font(BODY).color(p.muted).wrap()))
    };
    match &entry.row.action {
        AutomationAction::Prompt { prompt, to } => {
            let summary = match to {
                AutomationTarget::Session(id) => {
                    let link = session_link(p, "auto-recipient-link", entry.server.clone(), id, unknown_session(entry.recipient.clone()), BODY_MED);
                    sentence(p, t!("automations.do.prompt", session = SLOT), link)
                }
                AutomationTarget::Subagent(agent) => text(t!("automations.do.subagent", agent = agent)).font(BODY).color(p.text).wrap(),
            };
            block.child(summary).child(card(DO_BODY, prompt, false))
        }
        AutomationAction::Code { code, args } => {
            let block = block
                .child(summary(t!("automations.do.code"), Some(t!("automations.do.codeHint"))))
                .child(card(DO_BODY, code, true));
            match args {
                Some(args) => {
                    let pretty = serde_json::to_string_pretty(args).unwrap_or_else(|_| args.to_string());
                    block.child(card("auto-do-args", &pretty, true))
                }
                None => block,
            }
        }
        AutomationAction::Shell { command } => block
            .child(summary(t!("automations.do.shell"), Some(t!("automations.do.shellHint"))))
            .child(card(DO_BODY, command, true)),
    }
}

/// When it last ran and how often, anything unusual about that run, the session it woke unless
/// Do already names it, and what the run logged. Official shows the output whole in a box
/// (`view.tsx:200`), its first line a raw `Prompted ses_…`.
fn last_run_block(s: &App, p: &Pal, entry: &Entry) -> El {
    let info = &entry.row;
    let Some(at) = info.last_run_at else {
        return col().w_full().child(text(t!("automations.neverRun")).font(BODY).color(p.muted));
    };
    let mut summary = row()
        .w_full()
        .flex_wrap()
        .gap(4.0)
        .center_y()
        .child(text(relative_time(at, *s.now_ms)).font(BODY).color(p.text).help(format_date(at, &info.tz)))
        .child(text(format!("· {}", run_count(info.run_count))).font(BODY.features("\"tnum\"")).color(p.muted));
    // The status under the name already says a finished automation timed out.
    let timed_out = info.status == AutomationStatus::Done && info.last_outcome == Some(AutomationOutcome::Expired);
    if let Some(note) = outcome_note(info.last_outcome).filter(|_| !timed_out) {
        let ink = if info.last_outcome == Some(AutomationOutcome::Failed) { p.danger } else { p.muted };
        summary = summary.child(text("·").font(BODY).color(p.muted)).child(text(note).key("auto-last-outcome").font(BODY).color(ink));
    }
    let mut block = col().w_full().gap(8.0).child(summary);

    let (delivery, log) = info.last_output.as_deref().map_or((None, ""), split_output);
    if let Some(delivery) = delivery.filter(|d| !restates_do(&info.action, *d)) {
        let id = delivery.session();
        let link = session_link(p, "auto-last-session-link", entry.server.clone(), id, unknown_session(source::session_title(s, &entry.server, id)), BODY_MED);
        let template = match delivery {
            Delivery::Prompted(_) => t!("rune.automations.output.prompted", session = SLOT),
            Delivery::Subagent(_) => t!("rune.automations.output.subagent", session = SLOT),
        };
        block = block.child(sentence(p, template, link));
    }
    if !log.trim().is_empty() {
        block = block.child(body_card(s, p, &entry.key, "auto-last-output", log.trim_end(), true));
    }
    block
}

/// `12,480 runs`: the count grouped as the locale writes it.
fn run_count(count: u64) -> String {
    let template = opencode_i18n::plural_text(opencode_i18n::plural_key!("automations.runCount"), count);
    opencode_i18n::render(template, &[("count", &opencode_i18n::integer(count as i64))])
}

/// Whether a run's delivery line only repeats Do: a prompt to a session always wakes that session.
fn restates_do(action: &AutomationAction, delivery: Delivery<'_>) -> bool {
    matches!(
        (action, delivery),
        (AutomationAction::Prompt { to: AutomationTarget::Session(to), .. }, Delivery::Prompted(id)) if to == id
    )
}

/// A session the app can't name reads "another session" (view.tsx:AutomationInspector).
fn unknown_session(title: Option<String>) -> String {
    title.unwrap_or_else(|| t!("automations.session.unknown").to_owned())
}

/// Where a session's link goes in a translated sentence.
const SLOT: &str = "\u{1}";

/// A translated sentence with a link where its [`SLOT`] is, as `language.rich` builds it.
fn sentence(p: &Pal, template: String, link: El) -> El {
    let (before, after) = template.split_once(SLOT).unwrap_or((&template, ""));
    let words = |part: &str| (!part.is_empty()).then(|| text(part.to_owned()).font(BODY).color(p.text));
    row().w_full().flex_wrap().center_y().children(words(before)).child(link).children(words(after))
}
