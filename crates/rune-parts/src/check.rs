//! Errors an agent can act on before a note posts: unknown parts with a did-you-mean, kids a
//! part doesn't take, bad indents, and outlines too big for where they draw.

use std::fmt;

use crate::catalog::{CATALOG, Kids, names};
use crate::note::{Block, parse_note};
use crate::outline::{Node, indent_of, parse_outline, to_plain};

/// How much an outline may hold where it draws.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BudgetProfile {
    /// Words in the whole note, prose and outlines together (`None`: no limit).
    pub words: Option<usize>,
    /// Words on one line.
    pub leaf_words: usize,
    /// Frames nested in frames.
    pub depth: usize,
    /// Lines in one outline.
    pub nodes: usize,
}

impl BudgetProfile {
    /// A compact card: read at a glance.
    pub const CARD: BudgetProfile = BudgetProfile { words: Some(150), leaf_words: 22, depth: 3, nodes: 60 };
    /// A chat reply in Brocode: more room, still one screen.
    pub const CHAT: BudgetProfile = BudgetProfile { words: None, leaf_words: 32, depth: 4, nodes: 120 };
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Issue {
    /// 1-based line in what was checked.
    pub line: usize,
    pub msg: String,
}

impl fmt::Display for Issue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "line {}: {}", self.line, self.msg)
    }
}

/// Checks every ```` ```ui ```` outline in a note, with lines counted in the note.
pub fn check(note: &str, profile: &BudgetProfile) -> Vec<Issue> {
    let mut out = Vec::new();
    let mut words = 0;
    for b in parse_note(note, false) {
        match b {
            Block::Prose { text, .. } => words += text.split_whitespace().count(),
            Block::Ui { line, src, nodes, open } => {
                if open {
                    out.push(Issue { line, msg: "the ui fence is never closed".into() });
                }
                words += to_plain(&nodes).split_whitespace().count();
                out.extend(check_outline(&src, profile).into_iter().map(|i| Issue { line: i.line + line, ..i }));
            }
            Block::Fence { .. } => {}
        }
    }
    if let Some(max) = profile.words.filter(|&m| words > m) {
        out.push(Issue { line: 1, msg: format!("{words} words; a card holds {max}. Cut prose or outline lines") });
    }
    out
}

/// Checks one outline (a ```` ```ui ```` fence's body), lines counted in the outline.
pub fn check_outline(src: &str, profile: &BudgetProfile) -> Vec<Issue> {
    let mut out = Vec::new();
    for (i, l) in src.lines().enumerate().filter(|(_, l)| !l.trim().is_empty()) {
        if l.starts_with('\t') {
            out.push(Issue { line: i + 1, msg: "indent with spaces, not tabs".into() });
        } else if indent_of(l) % 2 == 1 {
            out.push(Issue { line: i + 1, msg: "indent by two spaces a level".into() });
        }
    }
    let nodes = parse_outline(src);
    if nodes.is_empty() {
        out.push(Issue { line: 1, msg: "the ui fence is empty".into() });
    }
    let mut count = 0;
    walk(&nodes, None, 0, profile, &mut count, &mut out);
    if count > profile.nodes {
        out.push(Issue { line: 1, msg: format!("{count} lines; the most is {}. Split it or show less", profile.nodes) });
    }
    out.sort_by_key(|i| i.line);
    out
}

fn walk(nodes: &[Node], parent: Option<&Node>, depth: usize, profile: &BudgetProfile, count: &mut usize, out: &mut Vec<Issue>) {
    for n in nodes {
        *count += 1;
        let issue = |msg: String| Issue { line: n.line, msg };
        let want = parent.map_or(0, |p| p.indent + 2);
        if n.indent != want && n.indent % 2 == 0 {
            out.push(issue(match parent {
                Some(p) => format!("indent two spaces under `{}` (line {})", p.part.name, p.line),
                None => "the first line of an outline isn't indented".into(),
            }));
        }
        let Some(spec) = n.spec() else {
            out.push(issue(unknown(&n.part.name)));
            walk(&n.kids, Some(n), depth, profile, count, out);
            continue;
        };
        if let Some(p) = parent.and_then(Node::spec)
            && let Kids::Only(allowed) = p.kids
            && !allowed.contains(&spec.name)
        {
            out.push(issue(format!("`{}` takes only {} under it", p.name, list(allowed))));
        }
        if spec.kids == Kids::None && !n.kids.is_empty() {
            out.push(issue(format!("`{}` takes nothing under it; put the indented lines in a `box`", spec.name)));
        }
        let depth = depth + usize::from(spec.container());
        if spec.container() && depth == profile.depth + 1 {
            out.push(issue(format!("frames nested {depth} deep; the most is {}", profile.depth)));
        }
        let words = n.words().split_whitespace().count();
        if words > profile.leaf_words {
            out.push(issue(format!("{words} words on one line; the most is {}", profile.leaf_words)));
        }
        let p = &n.part;
        match spec.name {
            "toggle" if !matches!(p.tone.as_deref(), Some("on" | "off")) => out.push(issue("`toggle` needs {on} or {off} before its label".into())),
            "say" if p.key.is_empty() => out.push(issue("`say` needs a speaker: `say Kit: words`".into())),
            "kv" if p.key.is_empty() || p.text.is_empty() => out.push(issue("`kv` is written `kv Label: value`".into())),
            "item" | "title" | "text" | "from" | "tools" | "chips" if p.text.is_empty() => out.push(issue(format!("`{}` needs words after it", spec.name))),
            _ => {}
        }
        walk(&n.kids, Some(n), depth, profile, count, out);
    }
}

fn list(names: &[&str]) -> String {
    names.iter().map(|n| format!("`{n}`")).collect::<Vec<_>>().join(", ")
}

/// `unknown part `buton`; did you mean `button`?`, or the whole list when nothing is close.
fn unknown(name: &str) -> String {
    let alias = match name {
        "said" | "chat" | "msg" => Some("say"),
        "chip" | "tag" | "tags" => Some("chips"),
        "frame" | "card" | "panel" | "group" => Some("box"),
        "window" => Some("screen"),
        "column" | "stack" => Some("col"),
        "heading" | "h" | "label" => Some("title"),
        "input" => Some("field"),
        "switch" => Some("toggle"),
        "divider" | "hr" | "line" => Some("rule"),
        "p" | "para" => Some("text"),
        _ => None,
    };
    let near = alias.or_else(|| {
        CATALOG.iter().map(|s| (distance(name, s.name), s.name)).filter(|&(d, s)| d <= 2.min(s.len() / 2)).min().map(|(_, s)| s)
    });
    match near {
        Some(s) => format!("unknown part `{name}`; did you mean `{s}`?"),
        None => format!("unknown part `{name}`; parts: {}", names()),
    }
}

fn distance(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.chars().enumerate() {
        let mut cur = vec![i + 1];
        for (j, &cb) in b.iter().enumerate() {
            cur.push((prev[j] + usize::from(ca != cb)).min(prev[j + 1] + 1).min(cur[j] + 1));
        }
        prev = cur;
    }
    prev[b.len()]
}
