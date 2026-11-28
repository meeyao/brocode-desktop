//! Rows for the long-prompt scenes (`--scene automations-long-prompt`): a prompt of many paragraphs, crons
//! past the single-time shapes, a delivery line with logs, and a server with no name.

use opencode_kit::model::{
    AutomationAction, AutomationInfo, AutomationOutcome, AutomationStatus, AutomationTarget,
    AutomationTime, AutomationTrigger,
};

/// A fixture project's ID and the folder an automation runs in.
type Place = (&'static str, &'static str);
const LUMEN: Place = ("6c756d656e000000", "/Users/demo/code/worktrees/lumen/reconnect-fix");
const HARBOR: Place = ("686172626f720000", "/Users/demo/code/harbor");
const OWNER: &str = "ses_fx0001aaaaaaaaaaaaaaaaaa";

pub(super) const DIRS: [&str; 2] = [LUMEN.1, HARBOR.1];
/// The row with the long prompt.
pub(super) const LONG_ID: &str = "aut_usage";

const LONG_PROMPT: &str = "Check the review bot's usage for the last four hours and compare it with the same window over the past week.\n\
\n\
1. Pull the request counts, token totals and error rates per repository from the usage dashboard.\n\
2. Flag any repository whose spend is more than twice its weekly median, and any whose error rate is above 2%.\n\
3. For each flagged repository, look at the five most expensive runs and say what they have in common: a large diff, a retry loop, a tool that keeps failing, or a prompt that grew.\n\
\n\
If nothing is flagged, reply with one line saying so and stop. Do not post a summary when there is nothing to act on.\n\
\n\
If something is flagged, write a short report:\n\
\n\
- One sentence on what changed and since when.\n\
- A table with repository, runs, tokens, cost, error rate and the weekly median beside each.\n\
- The likely cause, with links to two or three example runs.\n\
- What you would change, smallest first. Prefer a config change over a code change, and a code change over a new limit.\n\
\n\
Keep the report under 200 words. Use plain language, no headings beyond the table, and no emoji.\n\
\n\
Never pause the bot, change its limits or open a pull request from this automation. If a limit looks wrong, say which one and why, and wait for an answer.\n\
\n\
When the dashboard is unreachable, retry once after a minute. If it still fails, say that the check could not run and include the error text, so the next run is not read as a clean result.";

/// One paragraph with no line breaks, which only folds once its wrapped height is measured.
const PARAGRAPH: &str = "Total last month's model spend by project and compare it with the month before. Start from the billing export, not the dashboard, because the dashboard rounds each day and the rounding adds up over a month. Group the rows by project, then by model, and keep the request count beside the cost so a project that got cheaper per request but busier is not read as a regression. For any project whose spend moved by more than a fifth, say which model drove the change and whether the request count, the average prompt size or the price changed. If a project appears for the first time, list it apart from the rest with its first day of use. If a project stopped, say when its last request was. Close with the three projects that spent the most and the three that grew the most, in two short lists, and a single sentence on whether the total is on track for the quarter's budget. Keep everything in one message, and do not attach the export. When the export for the month is not complete yet, say which days are missing and stop there rather than comparing a partial month with a whole one, and try again on the next run.";

fn row(id: &str, name: &str, (project, dir): Place, trigger: AutomationTrigger, action: AutomationAction, now_ms: i64, age: i64) -> AutomationInfo {
    AutomationInfo {
        id: id.into(),
        name: name.into(),
        project_id: project.into(),
        session_id: Some(OWNER.into()),
        directory: dir.into(),
        trigger,
        tz: "America/Chicago".into(),
        action,
        expires: None,
        status: AutomationStatus::Active,
        next_due_at: None,
        run_count: 0,
        last_run_at: None,
        last_outcome: None,
        last_output: None,
        time: AutomationTime { created: now_ms - age * 86_400_000, updated: now_ms - age * 60_000 },
    }
}

fn cron(expr: &str) -> AutomationTrigger {
    AutomationTrigger::Cron { cron: expr.into(), tz: None }
}

fn prompt(text: &str, session: &str) -> AutomationAction {
    AutomationAction::Prompt { prompt: text.into(), to: AutomationTarget::Session(session.into()) }
}

pub(super) fn long_rows(now_ms: i64) -> Vec<AutomationInfo> {
    const HOUR: i64 = 3_600_000;
    vec![
        AutomationInfo {
            next_due_at: Some(now_ms + 3 * HOUR),
            run_count: 77,
            last_run_at: Some(now_ms - 55 * 60_000),
            last_outcome: Some(AutomationOutcome::Fired),
            last_output: Some(format!("Prompted {OWNER}")),
            ..row(LONG_ID, "Review bot usage check", LUMEN, cron("0 9,13,17,21 * * *"), prompt(LONG_PROMPT, OWNER), now_ms, 21)
        },
        AutomationInfo {
            next_due_at: Some(now_ms + 5 * HOUR),
            run_count: 9,
            last_run_at: Some(now_ms - 19 * HOUR),
            last_outcome: Some(AutomationOutcome::Fired),
            last_output: Some("Started subagent ses_fx0002aaaaaaaaaaaaaaaaaa\n3 dependencies behind, 1 with an advisory".into()),
            ..row(
                "aut_deps",
                "Dependency drift",
                LUMEN,
                cron("30 8 * * 1,3,5"),
                AutomationAction::Prompt {
                    prompt: "List the dependencies that are behind their latest release and note any with a security advisory.".into(),
                    to: AutomationTarget::Subagent("reviewer".into()),
                },
                now_ms,
                14,
            )
        },
        AutomationInfo {
            next_due_at: Some(now_ms + 40 * 60_000),
            run_count: 212,
            last_run_at: Some(now_ms - 80 * 60_000),
            last_outcome: Some(AutomationOutcome::Late),
            last_output: Some("Exit code: 0\nqueue depth 4, oldest 38s".into()),
            ..row(
                "aut_queue",
                "Queue depth probe",
                HARBOR,
                cron("0 */2 * * *"),
                AutomationAction::Shell { command: "bun scripts/queue-depth.ts --warn 50 --fail 200".into() },
                now_ms,
                30,
            )
        },
        AutomationInfo {
            next_due_at: Some(now_ms + 4 * 86_400_000),
            run_count: 3,
            last_run_at: Some(now_ms - 26 * 86_400_000),
            last_outcome: Some(AutomationOutcome::Fired),
            ..row(
                "aut_invoice",
                "Monthly cost roll-up",
                HARBOR,
                cron("0 9 1 * *"),
                prompt(PARAGRAPH, "ses_fx0006aaaaaaaaaaaaaaaaaa"),
                now_ms,
                90,
            )
        },
        AutomationInfo {
            next_due_at: Some(now_ms + 9 * 60_000),
            run_count: 1_204,
            last_run_at: Some(now_ms - 6 * 60_000),
            last_outcome: Some(AutomationOutcome::Skipped),
            // A session this window hasn't loaded: its link shows the ID cut short.
            session_id: Some("ses_fx9999zzzzzzzzzzzzzzzzzz".into()),
            ..row(
                "aut_cache",
                "Cache warm check",
                HARBOR,
                cron("*/15 6-22 * * 1-5"),
                AutomationAction::Shell { command: "bun scripts/cache-warm.ts".into() },
                now_ms,
                45,
            )
        },
        AutomationInfo {
            status: AutomationStatus::Paused,
            run_count: 18,
            last_run_at: Some(now_ms - 2 * 86_400_000),
            last_outcome: Some(AutomationOutcome::Failed),
            last_output: Some("The session that authorized this automation's tools no longer exists".into()),
            ..row(
                "aut_flaky",
                "Flaky test triage",
                LUMEN,
                AutomationTrigger::Every { every: "6 hours".into(), tz: None },
                AutomationAction::Code {
                    code: "const runs = await tools.ci.recent({ branch: \"main\", limit: 20 })\nconst flaky = runs.filter((run) => run.retried && run.passed)\nif (flaky.length > 3) return { send: { prompt: `Triage ${flaky.length} flaky runs on main.` } }".into(),
                    args: None,
                },
                now_ms,
                12,
            )
        },
        AutomationInfo {
            status: AutomationStatus::Done,
            session_id: None,
            run_count: 1,
            last_run_at: Some(now_ms - 5 * 86_400_000),
            last_outcome: Some(AutomationOutcome::Expired),
            ..row(
                "aut_done",
                "Pre-cut readiness check",
                LUMEN,
                AutomationTrigger::At { at: "2026-11-26T09:00".into(), tz: None },
                AutomationAction::Shell { command: "bun scripts/check-release.ts".into() },
                now_ms,
                6,
            )
        },
    ]
}
