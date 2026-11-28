//! Experimental › Sidebar sections' saved shape (`sidebar.sections`, one row per window): named,
//! collapsible sections of side tabs, each tab in at most one.
//!
//! The list is one order: tabs in no section and sections stand anywhere in it, a section where
//! its first tab is in the window's tab order. Every change writes the tab order back as the list
//! shows it (`items`, then `split`), so a section's tabs stay together and ⌘1–9, Ctrl+Tab and the
//! setting turned off keep the order a person sees. A row saved before (`v` missing) listed every
//! section above the other tabs: `legacy_order` reads it so.

use serde_json::{Value, json};

/// Shared vertical geometry of the sections list (matching `TabStrip`'s 28 pt rows and 3 pt gap).
pub const ROW_H: f32 = 28.0;
pub const REACH: f32 = 1.5;
pub const SLOT_H: f32 = ROW_H + REACH * 2.0;
pub const HEAD_H: f32 = 26.0;
pub const HEAD_ABOVE: f32 = 12.0;
pub const LINE_AROUND: f32 = 6.0;

/// A section: its name, whether it's folded, and its tabs (`Tab::uid`) in the order it shows them.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Section {
    pub id: String,
    pub name: String,
    pub collapsed: bool,
    pub members: Vec<String>,
}

/// Every section of a window, top to bottom.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Doc {
    pub sections: Vec<Section>,
}

impl Doc {
    pub fn from_json(v: &Value) -> Doc {
        let sections = v["sections"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|s| {
                        let id = s["id"].as_str()?.to_owned();
                        let members: Vec<String> = s["tabs"].as_array()?.iter().filter_map(|t| t.as_str().map(str::to_owned)).collect();
                        let name = s["name"].as_str().unwrap_or_default().to_owned();
                        Some(Section { id, name, collapsed: s["collapsed"].as_bool().unwrap_or(false), members })
                    })
                    .filter(|s| !s.members.is_empty())
                    .collect()
            })
            .unwrap_or_default();
        let mut doc = Doc { sections };
        doc.dedupe();
        doc
    }

    pub fn to_json(&self) -> Value {
        let sections: Vec<Value> = self.sections.iter().map(|s| json!({ "id": s.id, "name": s.name, "collapsed": s.collapsed, "tabs": s.members })).collect();
        json!({ "v": 2, "sections": sections })
    }

    pub fn get(&self, id: &str) -> Option<&Section> {
        self.sections.iter().find(|s| s.id == id)
    }

    pub fn get_mut(&mut self, id: &str) -> Option<&mut Section> {
        self.sections.iter_mut().find(|s| s.id == id)
    }

    /// The section tab `uid` is in.
    pub fn section_of(&self, uid: &str) -> Option<&Section> {
        self.sections.iter().find(|s| s.members.iter().any(|m| m == uid))
    }

    /// Takes `uids` out of every section; a section left empty goes.
    pub fn pull(&mut self, uids: &[String]) {
        for s in &mut self.sections {
            s.members.retain(|m| !uids.contains(m));
        }
        self.sections.retain(|s| !s.members.is_empty());
    }

    /// Keeps only the tabs still open; whether anything went.
    pub fn retain_open(&mut self, open: &[String]) -> bool {
        let before = self.clone();
        for s in &mut self.sections {
            s.members.retain(|m| open.contains(m));
        }
        self.sections.retain(|s| !s.members.is_empty());
        *self != before
    }

    /// `name`, or with " 2", " 3"… after it while another section has it (case aside).
    pub fn unique_name(&self, name: &str, own: &str) -> String {
        let taken = |n: &str| self.sections.iter().any(|s| s.id != own && s.name.eq_ignore_ascii_case(n));
        if !taken(name) {
            return name.to_owned();
        }
        (2..).map(|i| format!("{name} {i}")).find(|n| !taken(n)).unwrap_or_else(|| name.to_owned())
    }

    /// A file edited by hand could list a tab twice: it stays in its first section.
    fn dedupe(&mut self) {
        let mut seen: Vec<String> = Vec::new();
        for s in &mut self.sections {
            s.members.retain(|m| {
                let fresh = !seen.contains(m);
                seen.push(m.clone());
                fresh
            });
        }
        self.sections.retain(|s| !s.members.is_empty());
    }
}

/// Where dragged tab(s) land.
#[derive(Clone, Debug, PartialEq)]
pub enum Drop {
    /// Into section `id`: at its start when open (`prepend`), at its end when folded or from menu.
    Into { id: String, prepend: bool },
    /// Just before or after a tab, in whatever holds it (a section or the list).
    Before(String),
    After(String),
    /// In no section, just above a section's header.
    BeforeSection(String),
    /// In no section, at the end.
    End,
}

/// One thing at the list's top level: a tab in no section (`Tab::uid`), or a section.
#[derive(Clone, Debug, PartialEq)]
pub enum Item {
    Tab(String),
    Section(Section),
}

impl Item {
    /// Whether tab `uid` is this or in it.
    pub fn holds(&self, uid: &str) -> bool {
        match self {
            Item::Tab(u) => u == uid,
            Item::Section(s) => s.members.iter().any(|m| m == uid),
        }
    }

    pub fn is_section(&self, id: &str) -> bool {
        matches!(self, Item::Section(s) if s.id == id)
    }
}

/// The list's top level from `order` (every shown tab's uid in tab order): a section where its
/// first tab is, its tabs in its own order.
pub fn items(doc: &Doc, order: &[String]) -> Vec<Item> {
    let mut out = Vec::new();
    let mut placed: Vec<&str> = Vec::new();
    for u in order {
        match doc.section_of(u) {
            Some(sec) if !placed.contains(&sec.id.as_str()) => {
                placed.push(&sec.id);
                let mut sec = sec.clone();
                sec.members.retain(|m| order.contains(m));
                out.push(Item::Section(sec));
            }
            Some(_) => {}
            None => out.push(Item::Tab(u.clone())),
        }
    }
    out
}

/// `items` back to a doc (sections in list order, an empty one gone) and the tab order.
pub fn split(items: Vec<Item>) -> (Doc, Vec<String>) {
    let mut doc = Doc::default();
    let mut order = Vec::new();
    for it in items {
        match it {
            Item::Tab(u) => order.push(u),
            Item::Section(s) if !s.members.is_empty() => {
                order.extend(s.members.iter().cloned());
                doc.sections.push(s);
            }
            Item::Section(_) => {}
        }
    }
    (doc, order)
}

/// Removes empty sections in place so previewing `land` compares cleanly against `items`.
pub fn compact(items: &mut Vec<Item>) {
    items.retain(|it| !matches!(it, Item::Section(s) if s.members.is_empty()));
}

/// The tab order a row saved before `v` 2 showed: every section's tabs, then the rest.
pub fn legacy_order(doc: &Doc, order: &[String]) -> Vec<String> {
    let grouped = doc.sections.iter().flat_map(|s| s.members.iter()).filter(|m| order.contains(m)).cloned();
    grouped.chain(order.iter().filter(|u| doc.section_of(u).is_none()).cloned()).collect()
}

/// `uids` (in list order) leave where they were and land at `drop`.
pub fn land(items: &mut Vec<Item>, uids: &[String], drop: &Drop) {
    enum At {
        Top(usize),
        In(String, usize),
    }
    let carried = |it: &Item| matches!(it, Item::Tab(u) if uids.contains(u));
    // A place among what stays: the carried bare tabs before it no longer count.
    let top = |items: &[Item], i: usize| At::Top(i - items[..i].iter().filter(|it| carried(it)).count());
    let after = usize::from(matches!(drop, Drop::After(_)));
    let at = match drop {
        Drop::Into { id, prepend } => items.iter().any(|it| it.is_section(id)).then(|| At::In(id.clone(), if *prepend { 0 } else { usize::MAX })),
        Drop::Before(u) | Drop::After(u) => items.iter().enumerate().find_map(|(i, it)| match it {
            Item::Tab(t) if t == u => Some(top(items, i + after)),
            Item::Section(s) => s.members.iter().position(|m| m == u).map(|k| {
                let k = k + after;
                At::In(s.id.clone(), k - s.members[..k].iter().filter(|m| uids.contains(m)).count())
            }),
            Item::Tab(_) => None,
        }),
        Drop::BeforeSection(id) => items.iter().position(|it| it.is_section(id)).map(|i| top(items, i)),
        Drop::End => None,
    };
    items.retain(|it| !carried(it));
    for it in items.iter_mut() {
        if let Item::Section(s) = it {
            s.members.retain(|m| !uids.contains(m));
        }
    }
    match at {
        Some(At::In(id, k)) => {
            if let Some(Item::Section(s)) = items.iter_mut().find(|it| it.is_section(&id)) {
                let k = k.min(s.members.len());
                s.members.splice(k..k, uids.iter().cloned());
            }
        }
        Some(At::Top(i)) => {
            let i = i.min(items.len());
            items.splice(i..i, uids.iter().cloned().map(Item::Tab));
        }
        None => items.extend(uids.iter().cloned().map(Item::Tab)),
    }
    compact(items);
}

/// A new section `sec` of `uids` (in list order) where the first of them is: in its place, or
/// just after the section that held it.
pub fn gather(items: &mut Vec<Item>, uids: &[String], sec: Section) {
    let Some(first) = uids.first() else { return };
    let at = items.iter().position(|it| it.holds(first)).map_or(items.len(), |i| if matches!(items[i], Item::Section(_)) { i + 1 } else { i });
    let at = at - items[..at].iter().filter(|it| matches!(it, Item::Tab(u) if uids.contains(u))).count();
    items.retain(|it| !matches!(it, Item::Tab(u) if uids.contains(u)));
    for it in items.iter_mut() {
        if let Item::Section(s) = it {
            s.members.retain(|m| !uids.contains(m));
        }
    }
    items.insert(at.min(items.len()), Item::Section(Section { members: uids.to_vec(), ..sec }));
    compact(items);
}

/// Section `id` goes, its tabs left in its place.
pub fn dissolve(items: &mut Vec<Item>, id: &str) {
    if let Some(i) = items.iter().position(|it| it.is_section(id))
        && let Item::Section(s) = items.remove(i)
    {
        items.splice(i..i, s.members.into_iter().map(Item::Tab));
    }
}

/// `uids` out of their sections, each just below the section it left.
pub fn ungroup(items: &mut Vec<Item>, uids: &[String]) {
    let mut out = Vec::with_capacity(items.len());
    for it in std::mem::take(items) {
        match it {
            Item::Section(mut s) => {
                let (out_of, keep): (Vec<String>, Vec<String>) = s.members.into_iter().partition(|m| uids.contains(m));
                s.members = keep;
                if !s.members.is_empty() {
                    out.push(Item::Section(s));
                }
                out.extend(out_of.into_iter().map(Item::Tab));
            }
            tab => out.push(tab),
        }
    }
    *items = out;
}

/// Section `id` moved to index `to` among the list's top-level items (`0..items.len()`).
pub fn place_section(items: &mut Vec<Item>, id: &str, to: usize) {
    let Some(from) = items.iter().position(|it| it.is_section(id)) else { return };
    if from != to {
        let sec = items.remove(from);
        items.insert(to.min(items.len()), sec);
    }
}

/// One visible tab's resting position in `sidebar-sections-items` points.
#[derive(Clone, Debug, PartialEq)]
pub struct RowPos {
    pub uid: String,
    pub slot_top: f32,
    pub pill: [f32; 2],
}

/// One top-level block's resting position in `sidebar-sections-items` points.
#[derive(Clone, Debug, PartialEq)]
pub enum BlockPos {
    Tab(RowPos),
    Section {
        id: String,
        collapsed: bool,
        members: Vec<String>,
        block: [f32; 2],
        head: [f32; 2],
        rows: Vec<RowPos>,
        line: Option<f32>,
    },
}

impl BlockPos {
    pub fn span(&self) -> [f32; 2] {
        match self {
            BlockPos::Tab(r) => r.pill,
            BlockPos::Section { block, rows, collapsed, .. } => {
                if *collapsed { *block } else { [block[0], rows.last().map_or(block[1], |r| r.pill[1])] }
            }
        }
    }
}

/// Exact laid-out positions of every block, header, visible row and hairline in `items`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Layout {
    pub blocks: Vec<BlockPos>,
    pub total_h: f32,
}

impl Layout {
    pub fn tab(&self, uid: &str) -> Option<&RowPos> {
        self.blocks.iter().find_map(|b| match b {
            BlockPos::Tab(r) if r.uid == uid => Some(r),
            BlockPos::Section { rows, .. } => rows.iter().find(|r| r.uid == uid),
            BlockPos::Tab(_) => None,
        })
    }

    pub fn head_top(&self, id: &str) -> Option<f32> {
        self.blocks.iter().find_map(|b| match b {
            BlockPos::Section { id: s, head, .. } if s == id => Some(head[0]),
            _ => None,
        })
    }

    pub fn block_span(&self, id: &str) -> Option<[f32; 2]> {
        self.blocks.iter().find_map(|b| match b {
            BlockPos::Section { id: s, .. } if s == id => Some(b.span()),
            _ => None,
        })
    }

    pub fn line_y(&self, id: &str) -> Option<f32> {
        self.blocks.iter().find_map(|b| match b {
            BlockPos::Section { id: s, line, .. } if s == id => *line,
            _ => None,
        })
    }
}

/// Exact positions of `items` inside `sidebar-sections-items`, matching `view/sidebar/sections`.
pub fn measure(items: &[Item]) -> Layout {
    let mut blocks = Vec::with_capacity(items.len());
    let mut y = 0.0;
    let mut below: Option<f32> = None;
    for (n, it) in items.iter().enumerate() {
        match it {
            Item::Tab(uid) => {
                let slot_top = y;
                let pill = [y + REACH, y + REACH + ROW_H];
                y += SLOT_H;
                below = Some(REACH);
                blocks.push(BlockPos::Tab(RowPos { uid: uid.clone(), slot_top, pill }));
            }
            Item::Section(sec) => {
                let above = below.map_or(REACH, |b| HEAD_ABOVE - b);
                y += above;
                let block_top = y;
                let head = [y, y + HEAD_H];
                y += HEAD_H;
                let mut rows = Vec::new();
                if !sec.collapsed {
                    y += REACH;
                    for m in &sec.members {
                        let slot_top = y;
                        let pill = [y + REACH, y + REACH + ROW_H];
                        y += SLOT_H;
                        rows.push(RowPos { uid: m.clone(), slot_top, pill });
                    }
                    below = Some(REACH);
                } else {
                    below = Some(0.0);
                }
                let block_bottom = y;
                let line = if matches!(items.get(n + 1), Some(Item::Tab(_))) {
                    y += LINE_AROUND - below.unwrap_or(0.0);
                    let ly = y;
                    y += 1.0 + (LINE_AROUND - REACH);
                    below = None;
                    Some(ly)
                } else {
                    None
                };
                blocks.push(BlockPos::Section {
                    id: sec.id.clone(),
                    collapsed: sec.collapsed,
                    members: sec.members.clone(),
                    block: [block_top, block_bottom],
                    head,
                    rows,
                    line,
                });
            }
        }
    }
    Layout { blocks, total_h: y }
}

/// One thing the list shows at its top level, as indices into the window's tabs.
pub enum Entry {
    Tab(usize),
    Section(Section, Vec<usize>),
}

/// The tabs as the sidebar lists them, top to bottom.
pub struct Laid {
    pub entries: Vec<Entry>,
}

impl Laid {
    /// The tabs a reader sees, in order: a folded section's are hidden.
    pub fn visible(&self) -> impl Iterator<Item = usize> + '_ {
        self.entries.iter().flat_map(|e| match e {
            Entry::Tab(i) => std::slice::from_ref(i),
            Entry::Section(s, _) if s.collapsed => &[],
            Entry::Section(_, t) => t.as_slice(),
        }).copied()
    }

    /// Every listed tab, a folded section's too.
    pub fn all(&self) -> impl Iterator<Item = usize> + '_ {
        self.entries.iter().flat_map(|e| match e {
            Entry::Tab(i) => std::slice::from_ref(i),
            Entry::Section(_, t) => t.as_slice(),
        }).copied()
    }
}

/// Lays `shown` (the tabs the sidebar lists: their index and uid, in tab order) out by `doc`.
pub fn lay(doc: &Doc, shown: &[(usize, String)]) -> Laid {
    let at = |uid: &str| shown.iter().find(|(_, u)| u == uid).map(|(i, _)| *i);
    let mut entries = Vec::new();
    let mut placed: Vec<&str> = Vec::new();
    for (i, u) in shown {
        match doc.section_of(u) {
            Some(sec) if !placed.contains(&sec.id.as_str()) => {
                placed.push(&sec.id);
                entries.push(Entry::Section(sec.clone(), sec.members.iter().filter_map(|m| at(m)).collect()));
            }
            Some(_) => {}
            None => entries.push(Entry::Tab(*i)),
        }
    }
    Laid { entries }
}
