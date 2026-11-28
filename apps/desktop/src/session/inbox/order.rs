//! The inbox's rows in order, owned and kept while what they're read from holds: Dax's model
//! (`inbox_model`: `App::inbox_sections`, Recent ranked by the session list's times) read once
//! per change, not on every poll and every view. The view builds only the rows in sight
//! (`view::inbox`, a virtual list): each row is its server and session, and its card reads the
//! rest live (`App::inbox_entry_of`).

use std::cell::RefCell;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::ops::Range;
use std::rc::Rc;

use opencode_kit::servers::ServerKey;
use rune::Tracked;

use crate::App;
use crate::inbox_model::{Entry, RECENT_LIMIT, Sections};

/// Which of the inbox's groups a row is in, in the order they show.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Group {
    /// The open session when no group shows it: unheaded, first.
    Current,
    Needs,
    /// The pinned sessions, then the settled ones, under the Recent header (`opencode#66494`).
    Recent,
    /// The shelf below the list, or a group in it (`workingPlacement`).
    Working,
    /// Under a header that folds, shut at first.
    Archived,
}

impl Group {
    pub const ALL: [Group; 5] = [Group::Current, Group::Needs, Group::Recent, Group::Working, Group::Archived];
}

/// Which of the list's folds are open (`recentExpanded`, `workingExpanded`, `archivedExpanded`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Folds {
    pub recent: bool,
    pub working: bool,
    pub archived: bool,
}

impl From<crate::restore::InboxUi> for Folds {
    fn from(ui: crate::restore::InboxUi) -> Folds {
        Folds { recent: ui.recent_expanded, working: ui.working_expanded, archived: ui.archived_expanded }
    }
}

/// One row: its session, on its server.
#[derive(Clone, Debug, PartialEq)]
pub struct Shown {
    pub server: ServerKey,
    pub id: String,
    pub directory: String,
    pub working: bool,
    /// Its card's key (`ib-` and `Entry::key`), made once per reading.
    pub key: String,
}

/// The rows, grouped in order, and what shows around them.
#[derive(Debug, Default, PartialEq)]
pub struct Order {
    pub rows: Vec<Shown>,
    /// Where each group's rows are in `rows`, in [`Group::ALL`]'s order.
    pub groups: [Range<usize>; 5],
    /// How many of Recent's rows are pinned: they lead it, whatever Show more says.
    pub pinned: usize,
    /// Recent has more than it shows shut.
    pub more: bool,
    /// The open session's place in `rows`, in Recent or Archived: a fold that hides it (Recent
    /// shut shows its pins and its first [`RECENT_LIMIT`], Archived shut none) shows it as the
    /// current row.
    pub open: Option<usize>,
    /// The selected working session's place in `rows`, if any: kept visible in the collapsed
    /// Working section so the open session never drops out of the list (`opencode#66500`).
    pub selected_working: Option<usize>,
    /// Which reading this is: a new one has a new number.
    pub generation: u64,
}

impl Order {
    pub fn group(&self, g: Group) -> &[Shown] {
        &self.rows[self.groups[g as usize].clone()]
    }

    /// The places in `rows` group `g` shows under `folds`.
    pub fn shows(&self, g: Group, folds: Folds) -> Vec<usize> {
        let range = self.groups[g as usize].clone();
        match g {
            Group::Recent if !folds.recent => range.take(self.pinned + RECENT_LIMIT).collect(),
            // Collapsed Working still shows the selected session (`inbox.tsx:176-184`, `opencode#66500`).
            Group::Working if !folds.working => self.selected_working.into_iter().collect(),
            Group::Archived if !folds.archived => Vec::new(),
            Group::Current => {
                let folded = |at: &usize| !self.shows(Group::Recent, folds).contains(at) && !self.shows(Group::Archived, folds).contains(at);
                range.chain(self.open.filter(folded)).collect()
            }
            _ => range.collect(),
        }
    }

    /// Nothing to list, Working included.
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    fn of(app: &App, sections: &Sections<'_>) -> Order {
        let mut rows = Vec::new();
        let mut groups: [Range<usize>; 5] = Default::default();
        for (g, entries) in [&sections.current, &sections.needs, &sections.recent, &sections.working, &sections.archived].into_iter().enumerate() {
            let start = rows.len();
            rows.extend(entries.iter().map(shown));
            groups[g] = start..rows.len();
        }
        // Read with Recent and Archived open: shut, the open session they hide stands first, unheaded.
        let open = [(Group::Recent, &sections.recent), (Group::Archived, &sections.archived)]
            .into_iter()
            .find_map(|(g, entries)| entries.iter().position(|e| app.inbox_selected(e)).map(|k| groups[g as usize].start + k));
        let selected_working = sections
            .working
            .iter()
            .position(|e| app.inbox_selected(e))
            .map(|k| groups[Group::Working as usize].start + k);
        GENERATION.set(GENERATION.get() + 1);
        Order { rows, groups, pinned: sections.pinned, more: sections.more, open, selected_working, generation: GENERATION.get() }
    }
}

/// The row to open after `key` leaves the inbox: the nearest available one below it in `keys`
/// (the list as it showed before the row left), or the nearest one above when nothing below is
/// left (`packages/app/src/shell/titlebar/inbox-successor.ts`, `opencode#66558`).
pub fn inbox_successor(keys: &[String], key: &str, available: impl Fn(&str) -> bool) -> Option<String> {
    let index = keys.iter().position(|k| k == key)?;
    keys[index + 1..]
        .iter()
        .find(|k| available(k))
        .or_else(|| keys[..index].iter().rev().find(|k| available(k)))
        .cloned()
}

fn shown(e: &Entry<'_>) -> Shown {
    Shown { server: e.server.clone(), id: e.session.id.clone(), directory: e.session.directory.clone(), working: e.status == crate::inbox_model::InboxStatus::Working, key: format!("ib-{}", e.key()) }
}

thread_local! {
    static GENERATION: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

/// The order last read, and what it was read from.

#[derive(Default)]
pub struct Kept(RefCell<Option<(u64, Rc<Order>)>>);

impl App {
    /// Snapshots the inbox's visible row order while the open session is non-archived and visible,
    /// so archiving or deleting it opens the row below it rather than falling back to tab order
    /// (`packages/app/src/shell/titlebar/inbox.tsx:212-248`, `opencode#66558`).
    pub(crate) fn record_inbox_shown(&mut self) {
        if self.tab_layout() != crate::layout_state::TabLayout::Inbox {
            return;
        }
        let Some(current) = self.nav_current() else { return };
        let keys = self.nav_order();
        if !keys.contains(&current) {
            return;
        }
        let (server, sid) = self.inbox_target(&current);
        let archived = if server == *crate::servers::runtime::active(self) {
            self.session(&sid).is_some_and(|s| s.archived.is_some()) || self.inbox_archived().iter().any(|a| a.id == sid)
        } else {
            self.sidebar_inbox.servers.iter().find(|(s, _)| s == &server).is_some_and(|(_, rows)| rows.iter().any(|r| r.session.id == sid && r.session.archived.is_some()))
                || self.sidebar_inbox.archives.get(&server).is_some_and(|a| a.list.iter().any(|s| s.id == sid))
        };
        if archived && self.sidebar_inbox.shown.as_ref().is_some_and(|(k, _)| k == &current) {
            return;
        }
        self.sidebar_inbox.shown = Some((current, keys));
        let card_order = super::focus::visible_card_keys(self);
        self.sidebar_inbox.focus.note_order(card_order);
    }

    /// Picks the inbox successor row after `gone` (sessions on `server`) left the list
    /// (`inbox.tsx:226-248`, `opencode#66558`).
    pub(crate) fn pick_inbox_successor(&self, server: &ServerKey, gone: &[String]) -> Option<String> {
        if self.tab_layout() != crate::layout_state::TabLayout::Inbox {
            return None;
        }
        let (shown_key, shown_keys) = self.sidebar_inbox.shown.as_ref()?;
        let is_gone = |k: &str| {
            let (s, id) = self.inbox_target(k);
            s == *server && gone.iter().any(|g| g == &id)
        };
        if !is_gone(shown_key) {
            return None;
        }
        let visible: std::collections::HashSet<String> = self.nav_order().into_iter().collect();
        inbox_successor(shown_keys, shown_key, |k| visible.contains(k) && !is_gone(k))
    }

    /// Handles server-pushed `session.archived` / `session.deleted` removals in Inbox layout:
    /// if the removed session is currently open, opens its inbox successor before dropping its tab
    /// (`packages/app/src/shell/tabs/tabs.tsx:739-810`, `inbox.tsx:226-248`, `opencode#66558`).
    /// Returns `true` when this handler took over navigation for the removed session.
    pub(crate) fn on_server_inbox_removed(&mut self, server: &ServerKey, id: &str, archived: bool, tx: &mut rune::Tx) -> bool {
        if self.tab_layout() != crate::layout_state::TabLayout::Inbox {
            return false;
        }
        let open_gone = matches!(
            &*self.route,
            crate::Route::Session { id: open, server: on } if on == server && (open == id || self.root_of(open) == id)
        );
        let tab = self.tabs.iter().find(|t| t.is(server, id) && !t.is_draft()).map(crate::tabs::Tab::uid);
        if !open_gone {
            if let Some(uid) = tab {
                self.close_tab(&uid, tx);
            }
            return false;
        }
        let gone = [id.to_owned()];
        if let Some(next) = self.pick_inbox_successor(server, &gone) {
            self.open_inbox_session(&next, tx);
            if let Some(uid) = tab {
                self.close_tab(&uid, tx);
            } else {
                self.tabs.retain(|t| t.is_draft() || !t.is(server, id));
                self.save_tabs();
            }
            return true;
        }
        if archived {
            if let Some(uid) = tab {
                self.close_tab(&uid, tx);
            } else {
                self.go_home(tx);
            }
            return true;
        }
        false
    }

    /// The inbox's rows in order (`inbox_sections`), read again only when something they're read
    /// from changed.
    pub fn inbox_order(&self) -> Rc<Order> {
        let key = self.inbox_order_key();
        if let Some((k, order)) = &*self.sidebar_inbox.order.0.borrow()
            && *k == key
        {
            return order.clone();
        }
        let order = Rc::new(Order::of(self, &self.inbox_sections_with(true, true)));
        *self.sidebar_inbox.order.0.borrow_mut() = Some((key, order.clone()));
        order
    }

    /// Everything `inbox_sections` reads: the app's versions of what's tracked, and of the
    /// inbox's own state what a row's place depends on (another server's rows, the requests'
    /// ages, the archived list). The tick and the laid-out height move it every second while
    /// something works, so the inbox's state isn't read by version.
    fn inbox_order_key(&self) -> u64 {
        let mut h = DefaultHasher::new();
        (
            Tracked::version(&self.sessions),
            Tracked::version(&self.recent_page),
            Tracked::version(&self.running),
            Tracked::version(&self.requests),
            Tracked::version(&self.pending),
            self.kernel.slots.version(&crate::permission::PERMISSION_SENT),
            Tracked::version(&self.inboxes),
            Tracked::version(&self.projects),
            Tracked::version(&self.settings),
            self.kernel.slots.version(&crate::settings::SETTINGS_SCREEN),
            Tracked::version(&self.route),
            self.kernel.slots.version(&crate::session_menu::MENUS),
        )
            .hash(&mut h);
        self.kernel.slots.version(&crate::tabs::pins::PINS).hash(&mut h);
        crate::servers::runtime::active(self).hash(&mut h);
        crate::servers::runtime::version().hash(&mut h);
        let inbox = &*self.sidebar_inbox;
        // The requests' ages, whatever order the map keeps them in.
        inbox.asked.len().hash(&mut h);
        inbox.asked.iter().fold(0u64, |acc, kv| {
            let mut one = DefaultHasher::new();
            kv.hash(&mut one);
            acc ^ one.finish()
        }).hash(&mut h);
        self.inbox_archived().iter().map(|a| (&a.id, &a.title, a.archived)).for_each(|a| a.hash(&mut h));
        inbox.other_servers_hash(&mut h);
        h.finish()
    }
}
