//! What a screen reader hears of the agent's work without moving focus, through `Tx::announce`.
//!
//! The rule: never a reply's tokens, never silence. NVDA and Narrator queue polite UIA
//! live-region changes and speak each one whole, so every token as it streams would bury the
//! user; nothing at all would leave them guessing whether the agent heard. For the session on
//! screen the reader hears that work started, each tool as it starts (at most one every
//! [`TOOL_GAP`] seconds, never the same twice running), and the end with the reply's first
//! sentence; the reply itself is read at the user's pace in the timeline. A request that waits on
//! the user (a permission, a question) interrupts and, when focus was in the prompt it replaces,
//! takes focus; a failure interrupts; another tab's session finishing or asking is said too.
//!
//! None of it runs while no assistive technology reads the window (`Tx::assistive_technology`).

use rune::{LiveRegion, Tx};
use serde_json::Value;

use crate::App;
use opencode_kit::model::{Message, Part, Pending};

/// The least time between two tools said for one run, in seconds.
const TOOL_GAP: f64 = 4.0;
/// A reply's first sentence is cut here, in characters.
const SENTENCE_MAX: usize = 160;


/// The open session's run, as speech follows it.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum Turn {
    #[default]
    Idle,
    /// `said`: the last tool spoken, and when (the app's clock, seconds).
    /// `tools`: the open run's tool calls by id, and their tools' names (`session.tool.called`
    /// brings a call's input and `session.tool.input.started` its name).
    Working { session: String, said: Option<(String, f64)>, tools: Vec<(String, String)> },
}

impl Turn {
    fn tool_name(&self, session: &str, id: &str) -> Option<&str> {
        match self {
            Self::Working { session: s, tools, .. } if s == session => {
                tools.iter().find(|(c, _)| c == id).map(|(_, n)| n.as_str())
            }
            _ => None,
        }
    }
}

/// What the stream said, as speech hears it.
#[derive(Clone, Debug, PartialEq)]
pub enum Heard {
    Started { session: String },
    ToolInputStarted { session: String, id: String, name: String },
    Tool { session: String, step: String, now: f64 },
    Succeeded { session: String, reply: Option<String> },
    Failed { session: String, error: Option<String> },
    Interrupted { session: String },
    /// Another tab's session asks for the user, in words.
    AskedElsewhere { words: String },
    /// Another tab's session's run ended: its title and how.
    Elsewhere { title: String, failed: bool },
}

/// One thing to say.
#[derive(Clone, Debug, PartialEq)]
pub struct Say {
    pub words: String,
    pub how: LiveRegion,
}

/// The speech machine and what waits to be said on the next poll.
#[derive(Debug, Default)]
pub struct Speech {
    /// A request arrived on the open page: the key that takes focus, when focus is in the prompt
    /// the dock replaces, or nowhere. A keyboard user's, with or without a screen reader.
    focus_dock: Option<String>,
    /// Whether an assistive client reads the window, and the speech state kept while it does.
    mode: Mode,
}

pub(crate) static SPEECH: opencode_kernel::Slot<Speech> = opencode_kernel::Slot::new();

#[derive(Debug, Default)]
enum Mode {
    #[default]
    Silent,
    Listening(Box<Listening>),
}

enum ModeEvent {
    Assistive(bool),
}

impl Mode {
    fn step(&mut self, event: ModeEvent) {
        match (&self, event) {
            (Self::Silent, ModeEvent::Assistive(true)) => *self = Self::Listening(Box::default()),
            (Self::Listening(_), ModeEvent::Assistive(false)) => *self = Self::Silent,
            _ => {}
        }
    }
}

#[derive(Debug, Default)]
struct Listening {
    turn: Turn,
    queued: Vec<Say>,
    /// Focus was in the prompt, or nowhere, at the last poll: a request that replaces the prompt
    /// may take it.
    in_prompt: bool,
    /// The request the open page's dock showed at the last poll.
    dock: Option<String>,
    /// The focused terminal's output, as speech follows it.
    terminal: Output,
}

/// A focused terminal's new output: detached while unfocused; attached once focused (output before
/// the user types is read by moving through it, not said); watching after the first key (tracking
/// command rows so they are never said back); pending while new lines wait for [`OUTPUT_QUIET`]
/// seconds of silence before being spoken.
#[derive(Debug, Default)]
enum Output {
    #[default]
    Detached,
    Attached {
        terminal: (String, String),
        seen: Vec<(u128, u64)>,
    },
    Watching {
        terminal: (String, String),
        /// The rows the user typed on (their commands), which are never said back.
        input: Vec<u128>,
        seen: Vec<(u128, u64)>,
    },
    Pending {
        terminal: (String, String),
        input: Vec<u128>,
        seen: Vec<(u128, u64)>,
        lines: Vec<String>,
        at: f64,
    },
}

enum OutputEvent<'a> {
    Blurred,
    Poll {
        terminal: (String, String),
        snap: &'a terminal::TerminalSnapshot,
        now: f64,
    },
    Key {
        wk: &'a str,
        id: &'a str,
        cursor_row: Option<u128>,
    },
}

enum OutputEffect {
    WakeAt(f64),
    Say(String),
}

impl Output {
    fn step(&mut self, event: OutputEvent<'_>) -> Option<OutputEffect> {
        match event {
            OutputEvent::Blurred => {
                *self = Self::Detached;
                None
            }
            OutputEvent::Key { wk, id, cursor_row } => {
                match std::mem::take(self) {
                    Self::Attached { terminal, seen } if terminal.0 == wk && terminal.1 == id => {
                        let mut input = Vec::new();
                        record_input(&mut input, cursor_row);
                        *self = Self::Watching { terminal, input, seen };
                    }
                    Self::Watching { terminal, mut input, seen } if terminal.0 == wk && terminal.1 == id => {
                        record_input(&mut input, cursor_row);
                        *self = Self::Watching { terminal, input, seen };
                    }
                    Self::Pending { terminal, mut input, seen, lines, at } if terminal.0 == wk && terminal.1 == id => {
                        record_input(&mut input, cursor_row);
                        *self = Self::Pending { terminal, input, seen, lines, at };
                    }
                    other => *self = other,
                }
                None
            }
            OutputEvent::Poll { terminal: here, snap, now } => {
                let all_seen = || snap.screen_rows.iter().map(|r| (r.id, r.content_hash)).collect();
                let (terminal, input, seen, mut lines, prev_at) = match std::mem::take(self) {
                    Self::Attached { terminal, .. } if terminal == here => {
                        *self = Self::Attached { terminal, seen: all_seen() };
                        return None;
                    }
                    Self::Watching { terminal, input, seen } if terminal == here => (terminal, input, seen, Vec::new(), None),
                    Self::Pending { terminal, input, seen, lines, at } if terminal == here => (terminal, input, seen, lines, Some(at)),
                    _ => {
                        // A terminal newly focused: what it shows now is read by moving through it, not said.
                        *self = Self::Attached { terminal: here, seen: all_seen() };
                        return None;
                    }
                };
                if snap.alt_screen || !snap.scrollbar.at_bottom {
                    *self = Self::Watching { terminal, input, seen: all_seen() };
                    return None;
                }
                let cursor = usize::from(snap.cursor.row);
                let mut any_new = false;
                for (k, row) in snap.screen_rows.iter().enumerate() {
                    // A row that holds something new (a blank row filled, a new row scrolled in), once the
                    // cursor has left it (a program may still be writing it), and not a command the user typed.
                    if k == cursor || input.contains(&row.id) || seen.contains(&(row.id, row.content_hash)) {
                        continue;
                    }
                    let line = row.trimmed_text().trim();
                    if !line.is_empty() {
                        lines.push(line.to_owned());
                    }
                    any_new = true;
                }
                let seen = snap.screen_rows.iter().enumerate().filter(|&(k, _)| k != cursor).map(|(_, r)| (r.id, r.content_hash)).collect();
                if lines.is_empty() {
                    *self = Self::Watching { terminal, input, seen };
                    return None;
                }
                let at = if any_new { now } else { prev_at.unwrap_or(now) };
                if now - at < OUTPUT_QUIET {
                    *self = Self::Pending { terminal, input, seen, lines, at };
                    return Some(OutputEffect::WakeAt(at + OUTPUT_QUIET));
                }
                let words = if lines.len() <= OUTPUT_LINES {
                    lines.join(". ")
                } else {
                    opencode_i18n::t!("rune.a11y.terminal.output", count = lines.len(), last = lines.last().map_or("", String::as_str))
                };
                *self = Self::Watching { terminal, input, seen };
                Some(OutputEffect::Say(words))
            }
        }
    }
}

fn record_input(input: &mut Vec<u128>, cursor_row: Option<u128>) {
    if let Some(row_id) = cursor_row
        && !input.contains(&row_id)
    {
        input.push(row_id);
        let extra = input.len().saturating_sub(16);
        input.drain(..extra);
    }
}

/// How long a terminal's output must pause before its new lines are said, in seconds.
const OUTPUT_QUIET: f64 = 0.4;
/// More new lines than this are said as how many, and the last.
const OUTPUT_LINES: usize = 5;

impl Speech {
    /// Queues `words` for the next poll (a toast's message).
    pub fn say(&mut self, words: String, how: LiveRegion) {
        if let Mode::Listening(l) = &mut self.mode
            && !words.trim().is_empty()
        {
            l.queued.push(Say { words, how });
        }
    }

    /// The last thing queued interrupts rather than waits (a failure's toast).
    pub fn urgent(&mut self) {
        if let Mode::Listening(l) = &mut self.mode
            && let Some(say) = l.queued.last_mut()
        {
            say.how = LiveRegion::Assertive;
        }
    }

    fn tool_name(&self, session: &str, id: &str) -> Option<&str> {
        match &self.mode {
            Mode::Listening(l) => l.turn.tool_name(session, id),
            Mode::Silent => None,
        }
    }
}

/// One step of the machine: the next state and what to say, if anything.
pub fn step(turn: Turn, heard: Heard) -> (Turn, Option<Say>) {
    let polite = |words: String| Some(Say { words, how: LiveRegion::Polite });
    let assertive = |words: String| Some(Say { words, how: LiveRegion::Assertive });
    match (turn, heard) {
        (_, Heard::Started { session }) => (Turn::Working { session, said: None, tools: Vec::new() }, polite(opencode_i18n::t!("rune.a11y.working").to_owned())),
        (Turn::Working { session, said, mut tools }, Heard::ToolInputStarted { session: s, id, name }) if s == session => {
            tools.push((id, name));
            (Turn::Working { session, said, tools }, None)
        }
        (Turn::Working { session, said, tools }, Heard::Tool { session: s, step, now }) if s == session => {
            let quiet = said.as_ref().is_some_and(|(last, at)| *last == step || now - at < TOOL_GAP);
            if quiet {
                return (Turn::Working { session, said, tools }, None);
            }
            (Turn::Working { session, said: Some((step.clone(), now)), tools }, polite(step))
        }
        (Turn::Working { session, .. }, Heard::Succeeded { session: s, reply }) if s == session => {
            let words = match reply {
                Some(r) => opencode_i18n::t!("rune.a11y.replied", reply = r),
                None => opencode_i18n::t!("rune.a11y.done").to_owned(),
            };
            (Turn::Idle, polite(words))
        }
        (Turn::Working { session, .. }, Heard::Failed { session: s, error }) if s == session => {
            let error = error.unwrap_or_else(|| opencode_i18n::t!("notification.session.error.fallbackDescription").to_owned());
            (Turn::Idle, assertive(opencode_i18n::t!("rune.a11y.failed", error = error)))
        }
        (Turn::Working { session, .. }, Heard::Interrupted { session: s }) if s == session => {
            (Turn::Idle, polite(opencode_i18n::t!("rune.a11y.stopped").to_owned()))
        }
        (turn, Heard::AskedElsewhere { words }) => (turn, assertive(words)),
        (turn, Heard::Elsewhere { title, failed }) => {
            let words = if failed { opencode_i18n::t!("sidebar.inbox.announce.failed", title = title) } else { opencode_i18n::t!("sidebar.inbox.announce.done", title = title) };
            (turn, polite(words))
        }
        (turn, _) => (turn, None),
    }
}

/// Feeds one stream event to the machine (from `App::react`).
pub fn event(app: &mut App, ty: &str, data: &Value, sid: &str) {
    if matches!(ty, "permission.asked" | "form.created") && open_root(app).is_some_and(|r| r == app.root_of(sid)) {
        let key = if ty == "permission.asked" {
            session_view::dock::permission::PERMISSION_FOCUS.to_owned()
        } else {
            session_view::dock::question::initial_focus(app)
        };
        app.kernel.slots.get_mut(&SPEECH).focus_dock = Some(key);
    }
    if !listening(app) {
        return;
    }
    if crate::flag("--log-speech") && (ty.starts_with("session.") || ty.ends_with(".asked") || ty.starts_with("form.")) {
        eprintln!("speech: {ty} session {sid:?} open {:?}", open_root(app));
    }
    let Some(heard) = hear(app, ty, data, sid) else { return };
    if let Mode::Listening(l) = &mut app.kernel.slots.get_mut(&SPEECH).mode {
        let (turn, say) = step(std::mem::take(&mut l.turn), heard);
        l.turn = turn;
        l.queued.extend(say);
    }
}

/// Says what waits, and announces and focuses a request the open page's dock started showing
/// (from `poll`). A poll in one frame says everything queued in it as one announcement, so a
/// reader hears it all rather than only the last.
pub fn poll(app: &mut App, tx: &mut Tx) -> bool {
    let was = listening(app);
    poll_inner(app, tx);
    listening(app) != was
}

/// Whether an assistive client read the window at the last poll.
pub fn listening(app: &App) -> bool {
    matches!(app.kernel.slots.get(&SPEECH).mode, Mode::Listening(_))
}

fn poll_inner(app: &mut App, tx: &mut Tx) {
    // The dock is built with the next commit, or once a covered window builds again; focus
    // waits for its card however long that or its motion takes, and goes to it only from the
    // prompt it replaces or from nowhere.
    if app.kernel.slots.untracked(&SPEECH).focus_dock.is_some()
        && let Some(key) = app.kernel.slots.get_mut(&SPEECH).focus_dock.take()
        && matches!(tx.focused(), None | Some("prompt"))
    {
        tx.focus_when_shown(key, tx.focus_visible());
    }
    let is_listening = tx.assistive_technology();
    if !is_listening {
        if listening(app) {
            app.kernel.slots.get_mut(&SPEECH).mode.step(ModeEvent::Assistive(false));
        }
        return;
    }
    app.kernel.slots.get_mut(&SPEECH).mode.step(ModeEvent::Assistive(true));
    let shown = dock_request(app);
    if let Mode::Listening(l) = &mut app.kernel.slots.get_mut(&SPEECH).mode
        && shown.as_ref().map(|(id, _, _)| id) != l.dock.as_ref()
    {
        if let Some((_, words, focus)) = &shown {
            l.queued.push(Say { words: words.clone(), how: LiveRegion::Assertive });
            if l.in_prompt {
                tx.focus(focus.clone(), tx.focus_visible());
            }
        }
        l.dock = shown.map(|(id, _, _)| id);
    }
    terminal_output(app, tx);
    let queued = match &mut app.kernel.slots.get_mut(&SPEECH).mode {
        Mode::Listening(l) => std::mem::take(&mut l.queued),
        Mode::Silent => Vec::new(),
    };
    if !queued.is_empty() {
        let how = if queued.iter().any(|s| s.how == LiveRegion::Assertive) { LiveRegion::Assertive } else { LiveRegion::Polite };
        let words = queued.into_iter().map(|s| s.words).filter(|w| !w.trim().is_empty()).collect::<Vec<_>>().join(". ");
        if crate::flag("--log-speech") {
            eprintln!("speech: say {how:?} {words:?}");
        }
        tx.announce(words, how);
    }
    if let Mode::Listening(l) = &mut app.kernel.slots.get_mut(&SPEECH).mode {
        l.in_prompt = tx.focused().is_none_or(|k| k == "prompt");
    }
}

/// New output in the terminal that has focus, by the rule replies follow: not every byte, not
/// nothing. The rows a program prints (rows whose text is new, other than the cursor's, which
/// the user types on) are said once output pauses for [`OUTPUT_QUIET`]: up to
/// [`OUTPUT_LINES`] lines, else how many and the last. Nothing while a full-screen program draws
/// (the alternate screen), or while the user reads back up the scrollback.
fn terminal_output(app: &mut App, tx: &mut Tx) {
    let focused = tx.focused() == Some(crate::terminal::grid::SCREEN_KEY);
    let Some((wk, id)) = app.terminal_focus().filter(|_| focused) else {
        if let Mode::Listening(l) = &mut app.kernel.slots.get_mut(&SPEECH).mode {
            l.terminal.step(OutputEvent::Blurred);
        }
        return;
    };
    let Some(snap) = app.kernel.slots.get(&crate::terminal::TERMINAL).workspaces.get(&wk).and_then(|ws| ws.tabs.iter().find(|t| t.id == id)).map(|t| t.session.snapshot()) else { return };
    let now = *app.clock;
    if let Mode::Listening(l) = &mut app.kernel.slots.get_mut(&SPEECH).mode {
        match l.terminal.step(OutputEvent::Poll { terminal: (wk, id), snap: &snap, now }) {
            Some(OutputEffect::WakeAt(at)) => tx.wake_at(at),
            Some(OutputEffect::Say(words)) => l.queued.push(Say { words, how: LiveRegion::Polite }),
            None => {}
        }
    }
}

/// A key went to the focused terminal (from `terminal::key_terminal`): the row the cursor is on
/// is the user's command, and output from here on is said.
pub fn terminal_key(app: &mut App, wk: &str, id: &str) {
    if !listening(app) {
        return;
    }
    let Some(snap) = app.kernel.slots.get(&crate::terminal::TERMINAL).workspaces.get(wk).and_then(|ws| ws.tabs.iter().find(|t| t.id == id)).map(|t| t.session.snapshot()) else { return };
    let cursor_row = snap.screen_rows.get(usize::from(snap.cursor.row)).map(|r| r.id);
    if let Mode::Listening(l) = &mut app.kernel.slots.get_mut(&SPEECH).mode {
        l.terminal.step(OutputEvent::Key { wk, id, cursor_row });
    }
}

/// The request the open page's dock shows: its id, what a reader hears, and the key of what
/// takes focus.
fn dock_request(app: &App) -> Option<(String, String, String)> {
    if let Some(Pending::Permission { id, action, resources, .. }) = session_view::dock::permission::asked(app) {
        return Some((id, permission_words(&action, resources.first().map(String::as_str)), session_view::dock::permission::PERMISSION_FOCUS.to_owned()));
    }
    let form = session_view::dock::question::asking(app)?;
    let field = form.fields.first();
    let ask = field.map(|f| if f.description.is_empty() { f.title.as_str() } else { f.description.as_str() }).unwrap_or("");
    Some((form.id.clone(), format!("{}. {ask}", opencode_i18n::t!("notification.question.title")), session_view::dock::question::initial_focus(app)))
}

/// "Permission required. Run shell commands. git push origin main".
fn permission_words(action: &str, target: Option<&str>) -> String {
    let what = session_view::dock::permission::description(action).unwrap_or_else(|| crate::turns::tool_title(action));
    [opencode_i18n::t!("notification.permission.title"), what, target.unwrap_or("")].into_iter().filter(|w| !w.is_empty()).collect::<Vec<_>>().join(". ")
}

fn open_root(app: &App) -> Option<String> {
    match &*app.route {
        crate::Route::Session { id, .. } => Some(app.root_of(id).to_owned()),
        _ => None,
    }
}

fn hear(app: &App, ty: &str, data: &Value, sid: &str) -> Option<Heard> {
    let open = open_root(app).is_some_and(|r| r == sid);
    let session = sid.to_owned();
    let has_tab = || app.tabs.iter().any(|t| t.id == sid || t.id == app.root_of(sid));
    Some(match ty {
        "session.execution.started" if open => Heard::Started { session },
        "session.tool.input.started" => {
            let (id, name) = (data["id"].as_str()?, data["name"].as_str()?);
            Heard::ToolInputStarted { session, id: id.to_owned(), name: name.to_owned() }
        }
        "session.tool.called" if open => {
            let id = data["id"].as_str().unwrap_or("");
            let name = data["name"].as_str().or_else(|| app.kernel.slots.untracked(&SPEECH).tool_name(sid, id))?;
            Heard::Tool { session, step: crate::agent_rows::step_of(name, &data["input"]), now: *app.clock }
        }
        "session.execution.succeeded" if open => Heard::Succeeded { session, reply: first_sentence(app, sid) },
        "session.execution.failed" if open => Heard::Failed { session, error: data["error"]["message"].as_str().or(data["error"].as_str()).map(str::to_owned) },
        "session.execution.interrupted" if open => Heard::Interrupted { session },
        "session.execution.succeeded" | "session.execution.failed" if has_tab() => {
            let title = app.session(sid).map(|s| s.title.clone())?;
            Heard::Elsewhere { title, failed: ty == "session.execution.failed" }
        }
        // The open page's own requests are said as its dock shows them (`poll`).
        "permission.asked" if !open && has_tab() => {
            let target = data["resources"].as_array().and_then(|r| r.first()).and_then(Value::as_str);
            let title = app.session(sid).map(|s| s.title.clone()).unwrap_or_default();
            Heard::AskedElsewhere { words: format!("{title}: {}", permission_words(data["action"].as_str().unwrap_or(""), target)) }
        }
        "form.created" if !open && has_tab() => {
            let title = app.session(sid).map(|s| s.title.clone()).unwrap_or_default();
            Heard::AskedElsewhere { words: opencode_i18n::t!("sidebar.inbox.announce.input", title = title) }
        }
        _ => return None,
    })
}

/// The first sentence of the session's last reply, cut to [`SENTENCE_MAX`], without Markdown's
/// emphasis marks.
fn first_sentence(app: &App, sid: &str) -> Option<String> {
    let text = app.transcripts.get(sid)?.messages.iter().rev().find_map(|m| match m {
        Message::Assistant(a) => a.parts.iter().find_map(|p| match p {
            Part::Text { text } if !text.trim().is_empty() => Some(text.as_str()),
            _ => None,
        }),
        _ => None,
    })?;
    let line = text.lines().map(str::trim).find(|l| !l.is_empty())?.trim_start_matches(['#', '*', '-', '>', ' ']);
    let end = line.char_indices().find(|&(i, c)| matches!(c, '.' | '!' | '?') && line[i + c.len_utf8()..].starts_with(' ')).map_or(line.len(), |(i, c)| i + c.len_utf8());
    let sentence: String = line[..end].chars().filter(|c| !matches!(c, '*' | '`' | '_')).take(SENTENCE_MAX).collect();
    (!sentence.is_empty()).then_some(sentence)
}
