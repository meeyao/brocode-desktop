//! The `/` command popover over the composer, as in Desktop (`suggestions/machine.ts`,
//! `composer/model.ts`'s `slashCommands`): the server's commands for the composer's directory,
//! then the app's own (those with a `/name`), filtered by the draft's first word.
//! Picking an app command runs it at once; a server command fills in `/name `.

use std::cell::RefCell;

use opencode_kit::client::Server;
use rune::{Chord, Cx, El, Latest, TextEdit, Tx, Waker};
use serde_json::Value;
use session_view::palette::Pal;

use crate::suggestions::machine::Popover;
use crate::suggestions::popover::{PopoverRow, PopoverSpec, boundary, data_list, popup, suggest_search};
use crate::suggestions::{Suggest, SuggestionsHost};

#[derive(Clone, Debug, PartialEq)]
pub struct Command {
    pub name: String,
    pub description: String,
}

/// The app's `/` commands in the order Desktop registers them: the session's
/// (`use-session-commands.tsx:445-500`), Open file (`:515-522`), the terminal plugin after it
/// (`gui-plugins/src/terminal/renderer.tsx:90`), MCPs (`:655-664`), then the composer's
/// (`composer/commands.tsx:109-162`) and BTW (`gui-plugins/src/btw/renderer.tsx:47`).
const ORDER: [&str; 14] = ["new", "undo", "redo", "compact", "fork", "export", "open", "terminal", "mcp", "model", "connect", "reload", "agent", "btw"];

/// One app or plugin `/` command offered alongside the server's commands.
#[derive(Clone, Debug, PartialEq)]
pub struct BuiltinSlashCommand {
    pub id: &'static str,
    pub name: String,
    pub title: String,
    pub description: String,
    /// The strokes of its shortcut here: one for a plain shortcut, more for a chord, none unbound.
    pub keys: Vec<Chord>,
    /// An app or plugin command whose `/name` takes words (`slash.arguments`): picking it types
    /// `/name ` instead of running immediately.
    pub args: bool,
}

/// One row of the popover: a server command, or an app command (`builtin`, its id).
#[derive(Clone, Debug, PartialEq)]
pub struct Item {
    /// The row's id: the server command's name, or `builtin.<id>`.
    pub id: String,
    pub name: String,
    /// What the query matches besides the name: an app command's title.
    pub title: String,
    pub description: String,
    /// Its shortcut's strokes here, the user's own or its default: an app command's, or a plugin's.
    pub keys: Vec<Chord>,
    pub builtin: Option<&'static str>,
    /// An app command whose `/name` takes words (`slash.arguments`): picking it types `/name `.
    pub args: bool,
}

#[derive(Default)]
pub struct Slash {
    /// `/api/command` for `dir`.
    pub commands: Vec<Command>,
    dir: Option<String>,
    fetch: Latest<(String, Vec<Command>)>,
    projection: RefCell<Option<(Vec<Item>, opencode_search::SearchArena)>>,
}

impl Slash {
    pub fn set_waker(&mut self, waker: Waker) {
        self.fetch = Latest::new(waker);
    }

    /// Drops the cached directory mark when `command.updated` arrives for this directory or with
    /// no location (`packages/client/src/solid/data.ts:1522-1540`, `opencode#66813`), so
    /// `poll_commands` re-fetches `/api/command` immediately if open or on the next `/` open.
    pub fn invalidate(&mut self, dir: Option<&str>) {
        if dir.is_none() || self.dir.as_deref() == dir {
            self.dir = None;
        }
    }
}

/// Object-safe host boundary for `/` slash commands: catalog lookup, fuzzy ranking, server command
/// fetching, execution, and popover rendering.
pub trait SlashHost {
    /// Read-only access to the slash command cache/fetcher (must not dirty `Tracked` cells).
    fn slash(&self) -> &Slash;
    /// Mutable access to the slash command cache/fetcher.
    fn slash_mut(&mut self) -> &mut Slash;

    /// Read-only access to the composer suggestion state (used to read the active `/` query and
    /// highlight).
    fn slash_suggest(&self) -> &Suggest;

    /// Active composer editor (write), cleared or filled with `/name ` when a command is picked.
    fn slash_composer_mut(&mut self) -> &mut TextEdit;

    /// Directory of the active session or draft for `/api/command` queries.
    fn slash_directory(&self) -> String;
    /// Live OpenCode server connection, when connected.
    fn slash_live_server(&self) -> Option<Server> {
        None
    }

    /// Built-in app and plugin `/` commands currently offered by the host.
    fn builtin_slash_commands(&self) -> Vec<BuiltinSlashCommand> {
        Vec::new()
    }
    /// Executes a no-argument built-in or plugin command by `id`.
    fn run_builtin_command(&mut self, id: &str, tx: &mut Tx) {
        let _ = (id, tx);
    }
    /// Executes an argument-taking command by `id` with trimmed `args`.
    fn run_slash_command(&mut self, id: &str, args: &str, tx: &mut Tx) {
        let _ = (id, args, tx);
    }
    /// Refreshes suggestion list highlights after server slash commands land.
    fn refresh_slash_suggestions(&mut self, tx: &mut Tx) {
        let _ = tx;
    }
}

/// `/api/command`'s `{ data: [{ name, description }] }`, or the fixture's list.
pub fn commands(v: &Value) -> Vec<Command> {
    data_list(v)
        .filter_map(|c| {
            Some(Command {
                name: c["name"].as_str()?.to_owned(),
                description: c["description"].as_str().unwrap_or("").to_owned(),
            })
        })
        .collect()
}

/// `parseSlashCommand`: the name after a leading `/` and the trimmed rest.
pub fn parse_slash(text: &str) -> Option<(&str, &str)> {
    let rest = text.strip_prefix('/')?;
    let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
    Some((&rest[..end], rest[end..].trim()))
}

/// The query the open commands list filters by: the word after a draft's `/`, or what the
/// commands menu's search field holds.
pub fn slash_query(host: &dyn SlashHost) -> Option<String> {
    host.slash_suggest().state.commands().map(|l| l.query.clone())
}

/// Every `/` command, as Desktop lists them (`composer/model.ts:261-291`): the server's, then
/// the app's in Desktop's order ([`ORDER`]), each with its description only (none for
/// `/new` and `/terminal`) and its keys.
pub fn slash_list(host: &dyn SlashHost) -> Vec<Item> {
    let server = host.slash().commands.iter().map(|c| Item {
        id: c.name.clone(),
        name: c.name.clone(),
        title: c.name.clone(),
        description: c.description.clone(),
        keys: Vec::new(),
        builtin: None,
        args: false,
    });
    let mut own: Vec<Item> = host
        .builtin_slash_commands()
        .into_iter()
        .map(|c| Item {
            id: format!("builtin.{}", c.id),
            name: c.name,
            title: c.title,
            description: c.description,
            keys: c.keys,
            builtin: Some(c.id),
            args: c.args,
        })
        .collect();
    own.sort_by_key(|c| ORDER.iter().position(|n| *n == c.name).unwrap_or(ORDER.len()));
    server.chain(own).collect()
}

/// The commands the query finds by name or title, best first, as Desktop's `commandList`
/// filters them (`editor/interaction.ts:336-340`: fuzzysort over `trigger` and `title`); all
/// of them, in order, for no query.
pub fn slash_items(host: &dyn SlashHost) -> Vec<Item> {
    let Some(query) = slash_query(host) else { return Vec::new() };
    let list = slash_list(host);
    if query.is_empty() {
        return list;
    }
    let mut cache = host.slash().projection.borrow_mut();
    // Each key on its own row (`2 * i` the name, `2 * i + 1` the title), so a fuzzy match never
    // runs from one into the other (`co` isn't in "mcp" then "Toggle MCPs"), and a command
    // ranks by its better key, as fuzzysort's `keys` do.
    if cache.as_ref().is_none_or(|(last, _)| *last != list) {
        let mut arena = opencode_search::SearchArena::default();
        for row in &list {
            arena.push(&row.name);
            arena.push(&row.title);
        }
        *cache = Some((list, arena));
    }
    let Some((list, arena)) = cache.as_ref() else { return Vec::new() };
    let mut seen = vec![false; list.len()];
    arena
        .query(&query, list.len() * 2)
        .into_iter()
        .map(|r| r / 2)
        .filter(|&i| !std::mem::replace(&mut seen[i], true))
        .map(|i| list[i].clone())
        .collect()
}

pub fn slash_active(host: &dyn SlashHost) -> Option<String> {
    host.slash_suggest().state.commands().and_then(|l| l.active.id()).map(str::to_owned)
}

/// A picked command (`onSuggestionSelect`, then `draft.setText`): an app command that takes
/// no words clears the draft, unless it came from the commands menu, and runs; any other
/// fills in `/name `, before the draft's words when the menu picked it.
pub fn pick_command(host: &mut dyn SlashHost, id: &str, menu: bool, tx: &mut Tx) {
    let Some(item) = slash_list(host).into_iter().find(|c| c.id == id) else { return };
    if let Some(command) = item.builtin.filter(|_| !item.args) {
        if !menu {
            let composer = host.slash_composer_mut();
            let len = composer.text.len();
            composer.set_selection(0, len);
            composer.insert("");
        }
        host.run_builtin_command(command, tx);
        return;
    }
    let composer = host.slash_composer_mut();
    let draft = composer.text.trim().to_owned();
    let text = if menu && !draft.is_empty() {
        format!("/{} {draft}", item.name)
    } else {
        format!("/{} ", item.name)
    };
    let len = composer.text.len();
    composer.set_selection(0, len);
    composer.insert(&text);
}

/// A sent `/name` naming an app command and no server one runs it, and the draft clears; a
/// command that takes words runs with the rest (`parseClientSlashCommand`).
pub fn run_slash(host: &mut dyn SlashHost, text: &str, tx: &mut Tx) -> bool {
    let Some((name, words)) = parse_slash(text.trim()) else { return false };
    if host.slash().commands.iter().any(|c| c.name == name) {
        return false;
    }
    let Some(item) = slash_list(host).into_iter().find(|c| c.builtin.is_some() && c.name == name) else {
        return false;
    };
    if item.args {
        let Some(command) = item.builtin else { return false };
        let composer = host.slash_composer_mut();
        let len = composer.text.len();
        composer.set_selection(0, len);
        composer.insert("");
        host.run_slash_command(command, words, tx);
        return true;
    }
    if !words.is_empty() {
        return false;
    }
    pick_command(host, &item.id, false, tx);
    true
}

/// Takes in fetched commands, and asks for the composer directory's when a `/` needs them.
pub fn poll_commands(host: &mut dyn SlashHost, tx: &mut Tx) -> bool {
    let mut changed = false;
    if let Some((directory, list)) = host.slash().fetch.take()
        && host.slash().dir.as_deref() == Some(directory.as_str())
    {
        host.slash_mut().commands = list;
        changed = true;
        if host.slash_suggest().state.commands().is_some() {
            tx.instant();
            host.refresh_slash_suggestions(tx);
        }
    }
    if slash_query(host).is_none() {
        return changed;
    }
    let Some(server) = host.slash_live_server() else { return changed };
    let directory = host.slash_directory();
    if host.slash().dir.as_deref() == Some(directory.as_str()) {
        return changed;
    }
    let slash = host.slash_mut();
    slash.dir = Some(directory.clone());
    slash.fetch.request(directory, move |directory| {
        let path = format!("/api/command?location%5Bdirectory%5D={}", opencode_search::query::percent_encode(&directory));
        let list = server.get(&path).map(|v| commands(&v)).unwrap_or_default();
        (directory, list)
    });
    changed
}

/// A `/name args` draft naming a known command: the name and the arguments.
pub fn slash_command(host: &dyn SlashHost, text: &str) -> Option<(String, String)> {
    let (name, args) = parse_slash(text)?;
    host.slash().commands.iter().any(|c| c.name == name).then(|| (name.to_owned(), args.to_owned()))
}

fn prepare_commands_view(host: &dyn SlashHost, width: f32, boundary: Option<f32>) -> Option<(PopoverSpec, Vec<PopoverRow>, Option<String>, bool)> {
    slash_query(host)?;
    let items = slash_items(host);
    let active = slash_active(host);
    let rows = items
        .iter()
        .map(|c| PopoverRow {
            id: c.id.clone(),
            key: format!("command-{}", c.id),
            label: format!("/{}", c.name),
            lead: None,
            description: c.description.clone(),
            // A chord's strokes read with a space between them ("Ctrl+K Ctrl+C").
            trail: (!c.keys.is_empty()).then(|| c.keys.iter().map(|chord| opencode_ui::keymap::caps(*chord).join("+")).collect::<Vec<_>>().join(" ")),
        })
        .collect();
    let label = opencode_i18n::t!("ui.promptInput.commands");
    let has_search = matches!(host.slash_suggest().state, Popover::CommandMenu(_));
    let spec = PopoverSpec {
        key: "commands",
        label,
        empty: opencode_i18n::t!("ui.promptInput.noMatchingItems"),
        width,
        boundary,
        search: None,
    };
    Some((spec, rows, active, has_search))
}

/// The `/` popover over the composer: each command's `/name`, its description, and its keys
/// in words at the end (`editor.tsx:1400-1402`: `keybind.join("+")`, muted).
pub fn commands_view<H: SlashHost + SuggestionsHost + 'static>(s: &H, p: &Pal, cx: &Cx<H>) -> Option<El> {
    s.slash_suggest().state.commands()?;
    let width = cx.layout_size("composer").map_or(0.0, |[w, _]| w);
    let (mut spec, rows, active, has_search) = prepare_commands_view(s, width, boundary(cx))?;
    // The commands menu searches in a field of its own, its placeholder the slash (`editor.tsx:201-211`).
    if has_search {
        spec.search = Some(suggest_search::<H>(p, &s.slash_suggest().search, spec.label, "/"));
    }
    Some(popup::<H>(p, spec, rows, active.as_deref()))
}
