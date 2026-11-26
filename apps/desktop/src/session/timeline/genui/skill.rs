//! Generates the `rune-ui` agent skill from [`rune_parts::CATALOG`] and handles `--ui-skill` /
//! `--check-ui` before the window or GPU starts, so the skill text can never drift from the parser.

use std::fmt::Write as _;
use std::process::exit;

use rune_parts::{Block, BudgetProfile, CATALOG, Issue, Kids, check, check_outline, docs, parse_note};

/// Renders the complete `skills/rune-ui/SKILL.md` document directly from [`rune_parts::CATALOG`].
pub fn render() -> String {
    let budget = BudgetProfile::CHAT;
    let mut out = String::with_capacity(4096);

    writeln!(out, "---").ok();
    writeln!(out, "name: rune-ui").ok();
    writeln!(
        out,
        "description: \"Write ```ui outlines that render as native frames, rows, chat threads, key-value facts, statuses, and controls in Brocode replies. Use when showing a UI mockup, product flow, before-and-after layout, settings screen, or status summary to a user in Brocode.\""
    )
    .ok();
    writeln!(out, "---\n\n# Generative UI outlines (`ui`)\n").ok();
    writeln!(
        out,
        "In Brocode, a ```` ```ui ```` code fence in a chat reply renders as native UI parts instead of a raw code block. Surround it with ordinary Markdown prose, headings, or code fences as needed.\n"
    )
    .ok();
    writeln!(out, "## Rules\n").ok();
    writeln!(out, "- Every non-blank line begins with a part name from the catalog below.").ok();
    writeln!(out, "- Indent child lines by **2 spaces** per level (spaces only, never tabs; the first line is unindented).").ok();
    writeln!(
        out,
        "- Keep outlines compact: at most **{}** lines per outline, **{}** nested container frames, and **{}** words on one line.",
        budget.nodes, budget.depth, budget.leaf_words
    )
    .ok();
    writeln!(
        out,
        "- Never invent part names, pixel sizes, or hex colors. Color comes only from `{{tone}}` words:"
    )
    .ok();
    writeln!(out, "  - **Done (green):** `{{done}}`, `{{ship}}`, `{{merged}}`, `{{after}}`, `{{works}}`, `{{pass}}`, `{{fixed}}`, `{{live}}`, `{{landed}}`, `{{built}}`, `{{ok}}`, `{{yes}}`, `{{on}}`").ok();
    writeln!(out, "  - **Risk (red):** `{{risk}}`, `{{blocked}}`, `{{broken}}`, `{{bug}}`, `{{fail}}`, `{{error}}`, `{{warning}}`, `{{danger}}`, `{{revert}}`, `{{no}}`").ok();
    writeln!(out, "  - **Proposal (amber):** `{{proposal}}`, `{{hold}}`, `{{draft}}`, `{{plan}}`, `{{next}}`, `{{pending}}`, `{{waiting}}`, `{{todo}}`, `{{review}}`, `{{decide}}`").ok();
    writeln!(out, "  - **Working:** `{{working}}`, `{{running}}`, `{{building}}`").ok();
    writeln!(out, "  - **Accent:** `{{accent}}`, `{{primary}}`, `{{pick}}`, `{{recommended}}`\n").ok();
    writeln!(out, "## Parts catalog\n\n| Syntax | Children | Draws |\n| --- | --- | --- |").ok();
    for s in CATALOG {
        let kids = match s.kids {
            Kids::Any => "any part".to_owned(),
            Kids::None => "none".to_owned(),
            Kids::Only(allowed) => {
                let list = allowed.iter().map(|k| format!("`{k}`")).collect::<Vec<_>>().join(", ");
                format!("only {list}")
            }
        };
        let syntax = s.syntax.replace('|', "\\|");
        writeln!(out, "| `{syntax}` | {kids} | {} |", s.about).ok();
    }

    writeln!(out, "\n## Examples by part").ok();
    for s in CATALOG {
        writeln!(out, "\n### `{}` — {}\n\n```ui\n{}\n```", s.syntax, s.about, s.example).ok();
    }

    writeln!(out, "\n## Checking an outline\n").ok();
    writeln!(
        out,
        "Validate a reply or standalone outline from the command line (`0` when clean, `1` with line numbers and suggestions when invalid):\n"
    )
    .ok();
    writeln!(out, "~~~~sh\nopencode-desktop --check-ui - <<'EOF'\n```ui").ok();
    if let Some(first) = CATALOG.first() {
        writeln!(out, "{}", first.example).ok();
    }
    writeln!(out, "```\nEOF\n~~~~").ok();
    out
}

/// Checks either a full Markdown reply containing ```` ```ui ```` fences or a bare outline against
/// [`BudgetProfile::CHAT`].
pub fn check_input(input: &str) -> Vec<Issue> {
    let has_fence = parse_note(input, false).iter().any(|b| matches!(b, Block::Ui { .. }));
    if has_fence {
        check(input, &BudgetProfile::CHAT)
    } else {
        check_outline(input, &BudgetProfile::CHAT)
    }
}

/// Handles `--ui-skill [PART]` and `--check-ui FILE|-` before window/GPU startup.
pub fn maybe_cli() -> bool {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let Some(pos) = args.iter().position(|a| a == "--ui-skill") {
        let part = args.get(pos + 1).filter(|s| !s.starts_with('-')).map(String::as_str);
        match part {
            None => print!("{}", render()),
            Some(name) => match docs(Some(name)) {
                Ok(text) => print!("{text}"),
                Err(err) => {
                    eprintln!("opencode-desktop: {err}");
                    exit(1);
                }
            },
        }
        return true;
    }
    if let Some(pos) = args.iter().position(|a| a == "--check-ui") {
        let Some(path) = args.get(pos + 1) else {
            eprintln!("usage: opencode-desktop --check-ui <FILE|->");
            exit(2);
        };
        let src = if path == "-" {
            std::io::read_to_string(std::io::stdin())
        } else {
            std::fs::read_to_string(path)
        };
        let src = match src {
            Ok(s) => s,
            Err(err) => {
                eprintln!("opencode-desktop: {path}: {err}");
                exit(1);
            }
        };
        let issues = check_input(&src);
        for issue in &issues {
            println!("{issue}");
        }
        if !issues.is_empty() {
            exit(1);
        }
        return true;
    }
    false
}
