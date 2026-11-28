//! The connect dialog's steps around one integration's sign-in (Desktop's
//! `providers/connect/dialog-machine.ts`): the searchable list, a connected integration's
//! accounts, the sign-in, Claude Agent's setup, a Console workspace with no models, and the
//! models to start with once connected. `session` changes whenever the provider does, so a
//! result for a provider the dialog left is stale.

use opencode_kit::integration::MethodKind;

use crate::machine::{self as conn, Effect as ConnEffect, Event as ConnEvent, Screen, State as Conn};

/// The OpenCode Console account is the `opencode` integration.
pub const CONSOLE: &str = "opencode";
pub const CLAUDE: &str = "claude-agent";
/// `claude` gets 30 s to start and 30 more to read each model's context size.
pub const CLAUDE_CHECK_TIMEOUT_S: f64 = 65.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReturnTo {
    Picker,
    Accounts,
}

#[derive(Clone, Debug, PartialEq)]
pub enum AccountAction {
    Idle,
    Confirming(String),
    Renaming(String),
    Activating { attempt: u32, account: String },
    Removing { attempt: u32, account: String, last: bool },
}

#[derive(Clone, Debug, PartialEq)]
pub enum ClaudePhase {
    Idle,
    /// The connect request is out; `replacing` names the accounts it replaces once saved.
    Starting { check: u32, replacing: Vec<String> },
    Checking { check: u32 },
    /// Models showed; the integration is read again before they count as a working login.
    Confirming { check: u32 },
    TimedOut { check: u32 },
}

#[derive(Clone, Debug, PartialEq)]
pub enum Step {
    Accounts { action: AccountAction, connect: Conn },
    Connect { back: ReturnTo, copied: Option<(String, bool)>, connect: Conn },
    Claude { back: ReturnTo, phase: ClaudePhase, connect: Conn },
    /// A Console workspace with no models yet; `pending` when its providers hadn't loaded.
    NoModels { back: ReturnTo, pending: bool, connect: Conn },
    Models,
}

impl Step {
    pub fn connect(&self) -> Option<&Conn> {
        match self {
            Self::Accounts { connect, .. } | Self::Connect { connect, .. } | Self::Claude { connect, .. } | Self::NoModels { connect, .. } => Some(connect),
            Self::Models => None,
        }
    }

    fn with_connect(&self, c: Conn) -> Step {
        match self.clone() {
            Self::Accounts { action, .. } => Self::Accounts { action, connect: c },
            Self::Connect { back, copied, .. } => Self::Connect { back, copied, connect: c },
            Self::Claude { back, phase, .. } => Self::Claude { back, phase, connect: c },
            Self::NoModels { back, pending, .. } => Self::NoModels { back, pending, connect: c },
            Self::Models => Self::Models,
        }
    }

    fn back(&self) -> ReturnTo {
        match self {
            Self::Connect { back, .. } | Self::Claude { back, .. } | Self::NoModels { back, .. } => *back,
            _ => ReturnTo::Picker,
        }
    }
}

/// What the server lists for the integration, the active account first.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Accounts {
    pub count: usize,
    pub active: Option<String>,
    /// The active account needs a fresh sign-in (for Claude Agent: the latest check of `claude`).
    pub problem: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Provider {
    pub id: String,
    pub accounts: Accounts,
    pub step: Step,
    /// Closing: the last step stays while it goes, and takes no input.
    pub closed: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Dialog {
    pub session: u32,
    pub attempt: u32,
    /// Opened for one provider, with no way back to the list.
    pub standalone: bool,
    /// The user picked a model to start with.
    pub completed: bool,
    /// OpenAI connected through ChatGPT token sharing, so the plan's welcome follows the close.
    pub chatgpt_welcome: bool,
    pub provider: Option<Provider>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    Selected { provider: String, from_picker: bool, accounts: Accounts },
    Input(ConnEvent),
    Result { session: u32, provider: String, event: ConnEvent, catalog: Option<Snapshot> },
    AccountsSynced(Accounts),
    AccountAdd,
    RenameStarted(String),
    RenameClosed,
    RemovePrompted(String),
    RemoveCancelled,
    RemoveConfirmed(String),
    AccountSettled { session: u32, attempt: u32 },
    ActivateRequested(String),
    ClaudeCheck { replacing: Vec<String> },
    ClaudeStatus { problem: bool, models: bool },
    ClaudeTimedOut { session: u32, check: u32 },
    ClaudeConfirmed { session: u32, check: u32, problem: bool, has_models: bool },
    LinkCopied { session: u32, attempt_id: String, copied: bool },
    ModelConfirmed(String),
    Navigate(BackTarget),
    Disposed,
}

/// The catalogs after a refresh: whether the Console's workspace providers were still loading,
/// and whether the connection offers any models.
#[derive(Clone, Debug, PartialEq)]
pub struct Snapshot {
    pub pending: bool,
    pub has_models: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Effect {
    Connect { session: u32, provider: String, effect: Box<ConnEffect> },
    AccountActivate { session: u32, attempt: u32, provider: String, account: String },
    AccountRemove { session: u32, attempt: u32, provider: String, account: String },
    ClaudeReplace(Vec<String>),
    ClaudeTimer { session: u32, check: u32 },
    ClaudeConfirm { session: u32, check: u32 },
    /// Shows every model the connection offers in the model list.
    Reveal(String),
    Connected(String),
    Toast(String),
    /// Runs the composer, or a new session, with the model `provider:id`.
    Apply(String),
    Close,
}

pub type Transition = (Dialog, Vec<Effect>);

fn wrap(session: u32, provider: &str, effects: Vec<ConnEffect>) -> Vec<Effect> {
    effects
        .into_iter()
        .filter(|e| !matches!(e, ConnEffect::Complete { .. }))
        .map(|effect| Effect::Connect { session, provider: provider.to_owned(), effect: Box::new(effect) })
        .collect()
}

pub fn transition(d: &Dialog, event: Event) -> Transition {
    let same = |effects| (d.clone(), effects);
    if let Event::Result { session, provider, event, .. } = &event
        && !current(d, *session)
    {
        let (_, effects) = conn::transition(&Conn::Disposed, event.clone());
        return same(wrap(*session, provider, effects));
    }
    if event == Event::Disposed {
        let effects = d.provider.as_ref().map(|p| leave(d.session, p)).unwrap_or_default();
        return (Dialog { provider: None, ..d.clone() }, effects);
    }
    if d.provider.as_ref().is_some_and(|p| p.closed) {
        return same(vec![]);
    }
    if let Event::Selected { provider, from_picker, accounts } = &event {
        if d.provider.as_ref().is_some_and(|p| p.id == *provider) {
            return same(vec![]);
        }
        let session = d.session + 1;
        let (start, effects) = conn::start();
        let leaving = d.provider.as_ref().map(|p| leave(d.session, p)).unwrap_or_default();
        let step = entry(provider, *from_picker, accounts, start);
        let p = Provider { id: provider.clone(), accounts: accounts.clone(), step, closed: false };
        return (Dialog { session, provider: Some(p), ..d.clone() }, [leaving, wrap(session, provider, effects)].concat());
    }
    let Some(p) = &d.provider else { return same(vec![]) };
    match event {
        // A breadcrumb targets a specific step. The root may skip the intermediate methods or
        // accounts step; a stale provider crumb must not choose a different destination.
        Event::Navigate(BackTarget::Picker) => to_picker(d, p),
        Event::Navigate(target) if Some(target) == back_target(d) => back(d, p),
        Event::Navigate(_) => same(vec![]),
        Event::AccountsSynced(accounts) => synced(d, p, accounts),
        Event::Input(e) => match p.step.connect() {
            Some(c) => connect_transition(d, p, &p.step, conn::transition(c, e), None),
            None => same(vec![]),
        },
        Event::Result { event, catalog, .. } => {
            let Some(c) = p.step.connect() else {
                let (_, effects) = conn::transition(&Conn::Disposed, event);
                return same(wrap(d.session, &p.id, effects));
            };
            // The accounts list and Claude Agent's setup wait for the user, even with one method.
            let event = match event {
                ConnEvent::Loaded { attempt, catalog, .. } => {
                    ConnEvent::Loaded { attempt, catalog, manual: matches!(p.step, Step::Accounts { .. } | Step::Claude { .. }) }
                }
                e => e,
            };
            connect_transition(d, p, &p.step, conn::transition(c, event), catalog)
        }
        e => match &p.step {
            Step::Accounts { action, connect } => account(d, p, action, connect, e),
            Step::Claude { back, phase, connect } => claude(d, p, *back, phase, connect, e),
            Step::Models => models(d, p, e),
            Step::Connect { back, connect, .. } => match e {
                Event::LinkCopied { session, attempt_id, copied }
                    if session == d.session && conn::authorization(connect).is_some_and(|a| a.attempt == attempt_id) =>
                {
                    let step = Step::Connect { back: *back, copied: Some((attempt_id, copied)), connect: connect.clone() };
                    (with_step(d, p, step), vec![])
                }
                _ => same(vec![]),
            },
            Step::NoModels { .. } => same(vec![]),
        },
    }
}

fn with_step(d: &Dialog, p: &Provider, step: Step) -> Dialog {
    Dialog { provider: Some(Provider { step, ..p.clone() }), ..d.clone() }
}

fn current(d: &Dialog, session: u32) -> bool {
    d.provider.as_ref().is_some_and(|p| !p.closed) && d.session == session
}

/// The first step: a healthy connected provider picked from the list shows its accounts.
fn entry(provider: &str, from_picker: bool, accounts: &Accounts, connect: Conn) -> Step {
    let claude = provider == CLAUDE;
    if from_picker && accounts.count > 0 && !(claude && accounts.problem) {
        return Step::Accounts { action: AccountAction::Idle, connect };
    }
    fresh(provider, ReturnTo::Picker, connect)
}

fn fresh(provider: &str, back: ReturnTo, connect: Conn) -> Step {
    if provider == CLAUDE {
        return Step::Claude { back, phase: ClaudePhase::Idle, connect };
    }
    Step::Connect { back, copied: None, connect }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BackTarget {
    Accounts,
    Methods,
    Picker,
}

/// Where Back goes from the step; none when there's nowhere to go.
pub fn back_target(d: &Dialog) -> Option<BackTarget> {
    let p = d.provider.as_ref()?;
    let picker = (!d.standalone).then_some(BackTarget::Picker);
    match &p.step {
        Step::Models => None,
        Step::Accounts { .. } => picker,
        Step::NoModels { .. } => Some(BackTarget::Methods),
        Step::Claude { back, .. } => {
            if *back == ReturnTo::Accounts && p.accounts.count > 0 {
                Some(BackTarget::Accounts)
            } else {
                picker
            }
        }
        Step::Connect { back, connect, .. } => {
            let methods = conn::catalog(connect).map_or(0, |c| c.methods.len());
            if *back == ReturnTo::Accounts && p.accounts.count > 0 && (methods == 1 || matches!(connect, Conn::Choosing { .. })) {
                return Some(BackTarget::Accounts);
            }
            if (p.id == CONSOLE || methods > 1) && conn::sel(connect).is_some() {
                return Some(BackTarget::Methods);
            }
            picker
        }
    }
}

fn back(d: &Dialog, p: &Provider) -> Transition {
    let Some(target) = back_target(d) else { return (d.clone(), vec![]) };
    let Some(c) = p.step.connect() else { return (d.clone(), vec![]) };
    if target == BackTarget::Picker {
        return to_picker(d, p);
    }
    let (next, effects) = conn::transition(c, ConnEvent::Back);
    let effects = wrap(d.session, &p.id, effects);
    let step = match target {
        BackTarget::Accounts => Step::Accounts { action: AccountAction::Idle, connect: next },
        _ => Step::Connect {
            back: if matches!(p.step, Step::Accounts { .. }) { ReturnTo::Picker } else { p.step.back() },
            copied: None,
            connect: next,
        },
    };
    (with_step(d, p, step), effects)
}

fn to_picker(d: &Dialog, p: &Provider) -> Transition {
    if d.standalone {
        return (d.clone(), vec![]);
    }
    (Dialog { session: d.session + 1, provider: None, ..d.clone() }, leave(d.session, p))
}

/// Stops the sign-in the provider's step left running. Claude's check timer lands stale.
fn leave(session: u32, p: &Provider) -> Vec<Effect> {
    let Some(c) = p.step.connect() else { return vec![] };
    let (_, effects) = conn::transition(c, ConnEvent::Disposed);
    wrap(session, &p.id, effects)
}

fn synced(d: &Dialog, p: &Provider, accounts: Accounts) -> Transition {
    if p.accounts == accounts {
        return (d.clone(), vec![]);
    }
    let next = Provider { accounts, ..p.clone() };
    let step = match &p.step {
        Step::Accounts { action: AccountAction::Activating { .. } | AccountAction::Removing { .. }, .. } => p.step.clone(),
        Step::Accounts { action, connect } => after_accounts(&next, action.clone(), connect.clone()),
        s => s.clone(),
    };
    (Dialog { provider: Some(Provider { step, ..next }), ..d.clone() }, vec![])
}

/// Accounts that ran out, or a Claude Agent login that broke, leave the list for the setup.
fn after_accounts(p: &Provider, action: AccountAction, connect: Conn) -> Step {
    if p.accounts.count == 0 || (p.id == CLAUDE && p.accounts.problem) {
        return fresh(&p.id, ReturnTo::Picker, connect);
    }
    Step::Accounts { action, connect }
}

fn account(d: &Dialog, p: &Provider, action: &AccountAction, connect: &Conn, e: Event) -> Transition {
    let busy = matches!(action, AccountAction::Activating { .. } | AccountAction::Removing { .. });
    let set = |a: AccountAction| (with_step(d, p, Step::Accounts { action: a, connect: connect.clone() }), vec![]);
    let same = (d.clone(), vec![]);
    match e {
        Event::AccountSettled { session, attempt } => {
            let a = match action {
                AccountAction::Activating { attempt, .. } | AccountAction::Removing { attempt, .. } => *attempt,
                _ => return same,
            };
            if session != d.session || attempt != a {
                return same;
            }
            // Removing the last account disconnects the integration: the dialog is done.
            if matches!(action, AccountAction::Removing { last: true, .. }) && p.accounts.count == 0 {
                let step = Step::Accounts { action: AccountAction::Idle, connect: connect.clone() };
                return (Dialog { provider: Some(Provider { step, closed: true, ..p.clone() }), ..d.clone() }, vec![Effect::Close]);
            }
            (with_step(d, p, after_accounts(p, AccountAction::Idle, connect.clone())), vec![])
        }
        _ if busy => same,
        Event::RenameStarted(id) => set(AccountAction::Renaming(id)),
        Event::RenameClosed if matches!(action, AccountAction::Renaming(_)) => set(AccountAction::Idle),
        Event::RemovePrompted(id) => set(AccountAction::Confirming(id)),
        Event::RemoveCancelled if matches!(action, AccountAction::Confirming(_)) => set(AccountAction::Idle),
        Event::ActivateRequested(id) if p.accounts.active.as_deref() != Some(id.as_str()) => {
            let attempt = d.attempt + 1;
            let step = Step::Accounts { action: AccountAction::Activating { attempt, account: id.clone() }, connect: connect.clone() };
            let next = Dialog { attempt, ..with_step(d, p, step) };
            (next, vec![Effect::AccountActivate { session: d.session, attempt, provider: p.id.clone(), account: id }])
        }
        Event::RemoveConfirmed(id) if *action == AccountAction::Confirming(id.clone()) => {
            let attempt = d.attempt + 1;
            let last = p.accounts.count == 1;
            let step = Step::Accounts { action: AccountAction::Removing { attempt, account: id.clone(), last }, connect: connect.clone() };
            let next = Dialog { attempt, ..with_step(d, p, step) };
            (next, vec![Effect::AccountRemove { session: d.session, attempt, provider: p.id.clone(), account: id }])
        }
        Event::AccountAdd => {
            if p.id == CLAUDE {
                return (with_step(d, p, Step::Claude { back: ReturnTo::Accounts, phase: ClaudePhase::Idle, connect: connect.clone() }), vec![]);
            }
            let methods = conn::catalog(connect).map_or(0, |c| c.methods.len());
            if methods == 0 {
                return same;
            }
            let step = Step::Connect { back: ReturnTo::Accounts, copied: None, connect: connect.clone() };
            if methods > 1 {
                return (with_step(d, p, step), vec![]);
            }
            let result = conn::transition(connect, ConnEvent::MethodSelected(0));
            connect_transition(&with_step(d, p, step.clone()), p, &step, result, None)
        }
        _ => same,
    }
}

fn claude(d: &Dialog, p: &Provider, back: ReturnTo, phase: &ClaudePhase, connect: &Conn, e: Event) -> Transition {
    let set = |phase: ClaudePhase, effects: Vec<Effect>| (with_step(d, p, Step::Claude { back, phase, connect: connect.clone() }), effects);
    let same = (d.clone(), vec![]);
    match e {
        // Connecting again is what makes the server check `claude` again.
        Event::ClaudeCheck { replacing } => {
            if claude_checking(phase, connect) || matches!(connect, Conn::Loading { .. }) {
                return same;
            }
            let check = d.attempt + 1;
            let (next, effects) = conn::transition(connect, ConnEvent::MethodSelected(0));
            if next == *connect {
                return same;
            }
            let step = Step::Claude { back, phase: ClaudePhase::Starting { check, replacing }, connect: next };
            (Dialog { attempt: check, ..with_step(d, p, step) }, wrap(d.session, &p.id, effects))
        }
        Event::ClaudeStatus { problem, models } => match phase {
            ClaudePhase::Checking { check } | ClaudePhase::TimedOut { check } | ClaudePhase::Confirming { check } => {
                if problem {
                    return set(ClaudePhase::Idle, vec![]);
                }
                // A check that outlives the timeout can still finish.
                if matches!(phase, ClaudePhase::Confirming { .. }) || !models {
                    return same;
                }
                set(ClaudePhase::Confirming { check: *check }, vec![Effect::ClaudeConfirm { session: d.session, check: *check }])
            }
            _ => same,
        },
        Event::ClaudeTimedOut { session, check } if session == d.session && *phase == (ClaudePhase::Checking { check }) => {
            set(ClaudePhase::TimedOut { check }, vec![])
        }
        Event::ClaudeConfirmed { session, check, problem, has_models } if session == d.session && *phase == (ClaudePhase::Confirming { check }) => {
            if problem {
                return set(ClaudePhase::Idle, vec![]);
            }
            let (next, effects) = finish(d, p, has_models, None, ReturnTo::Picker);
            (next, [vec![Effect::Reveal(p.id.clone())], effects].concat())
        }
        _ => same,
    }
}

pub fn claude_checking(phase: &ClaudePhase, connect: &Conn) -> bool {
    matches!(phase, ClaudePhase::Checking { .. } | ClaudePhase::Confirming { .. }) || matches!(connect, Conn::Starting(_) | Conn::Refreshing(..))
}

fn models(d: &Dialog, p: &Provider, e: Event) -> Transition {
    match e {
        Event::ModelConfirmed(key) => {
            let next = Dialog { completed: true, provider: Some(Provider { closed: true, ..p.clone() }), ..d.clone() };
            (next, vec![Effect::Close, Effect::Apply(key)])
        }
        _ => (d.clone(), vec![]),
    }
}

/// Applies a sign-in transition to the step; a sign-in that completes moves the dialog on.
fn connect_transition(d: &Dialog, p: &Provider, step: &Step, (next, effects): conn::Transition, catalog: Option<Snapshot>) -> Transition {
    let complete = effects.iter().find_map(|e| match e {
        ConnEffect::Complete { method } => Some(method.clone()),
        _ => None,
    });
    let sharing = complete.as_ref().is_some_and(|m| p.id == "openai" && m.kind == MethodKind::Oauth && m.id.as_deref() == Some("chatgpt-token-sharing"));
    let marked = Dialog { chatgpt_welcome: d.chatgpt_welcome || sharing, ..d.clone() };
    let d = &marked;
    let out = wrap(d.session, &p.id, effects);
    if Some(&next) == step.connect() && complete.is_none() {
        return (d.clone(), out);
    }
    let moved = with_step(d, p, step.with_connect(next.clone()));
    if complete.is_none() {
        return (moved, out);
    }
    let notify = [out, vec![Effect::Connected(p.id.clone())]].concat();
    let back = if matches!(step, Step::Accounts { .. }) { ReturnTo::Picker } else { step.back() };
    if let Step::Claude { phase, .. } = step {
        // Each connect saves a new account, so the ones this check replaces go.
        let (check, replacing) = match phase {
            ClaudePhase::Starting { check, replacing } => (*check, replacing.clone()),
            _ => (d.attempt + 1, vec![]),
        };
        let step = Step::Claude { back, phase: ClaudePhase::Checking { check }, connect: next };
        let dialog = Dialog { attempt: d.attempt.max(check), ..with_step(d, p, step) };
        let replace = if replacing.is_empty() { vec![] } else { vec![Effect::ClaudeReplace(replacing)] };
        return (dialog, [notify, replace, vec![Effect::ClaudeTimer { session: d.session, check }]].concat());
    }
    // The model list shows only each family's newest by default: show what the connection unlocked.
    let reveal = [notify, vec![Effect::Reveal(p.id.clone())]].concat();
    let moved_p = moved.provider.clone().unwrap_or_else(|| p.clone());
    if catalog.as_ref().is_some_and(|c| c.pending) {
        let step = Step::NoModels { back, pending: true, connect: next };
        return (with_step(&moved, &moved_p, step), reveal);
    }
    let (dialog, effects) = finish(&moved, &moved_p, catalog.is_some_and(|c| c.has_models), Some(next), back);
    (dialog, [reveal, effects].concat())
}

/// As the TUI: every connection goes on to its models; a Console connection with none stays so
/// the workspace can be fixed.
fn finish(d: &Dialog, p: &Provider, has_models: bool, connect: Option<Conn>, back: ReturnTo) -> Transition {
    let toast = vec![Effect::Toast(p.id.clone())];
    if has_models {
        return (with_step(d, p, Step::Models), toast);
    }
    if p.id == CONSOLE
        && let Some(connect) = connect
    {
        return (with_step(d, p, Step::NoModels { back, pending: false, connect }), toast);
    }
    (Dialog { provider: Some(Provider { closed: true, ..p.clone() }), ..d.clone() }, [toast, vec![Effect::Close]].concat())
}

/// Which frame the step uses: the searchable list, the models, the accounts, or a provider step.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Frame {
    Picker,
    Models,
    Accounts,
    Provider,
}

pub fn frame(d: &Dialog) -> Frame {
    match d.provider.as_ref().map(|p| &p.step) {
        None => Frame::Picker,
        Some(Step::Models) => Frame::Models,
        Some(Step::Accounts { .. }) => Frame::Accounts,
        Some(_) => Frame::Provider,
    }
}

/// What a provider step's body shows.
#[derive(Clone, Debug, PartialEq)]
pub enum Body {
    Picker,
    Models,
    Accounts,
    Claude { checking: bool, can_check: bool, timed_out: bool },
    NoModels { pending: bool, refreshing: bool },
    ConsoleSignIn { copied: Option<bool> },
    Screen(Screen),
}

pub fn body(d: &Dialog) -> Body {
    let Some(p) = &d.provider else { return Body::Picker };
    match &p.step {
        Step::Models => Body::Models,
        Step::Accounts { .. } => Body::Accounts,
        _ if matches!(p.step.connect(), Some(Conn::Failed(..))) => Body::Screen(Screen::Failed),
        Step::Claude { phase, connect, .. } => Body::Claude {
            checking: claude_checking(phase, connect),
            can_check: !matches!(connect, Conn::Loading { .. }),
            timed_out: matches!(phase, ClaudePhase::TimedOut { .. }),
        },
        Step::NoModels { pending, connect, .. } => Body::NoModels { pending: *pending, refreshing: matches!(connect, Conn::Refreshing(..)) },
        Step::Connect { copied, connect, .. } => {
            let screen = conn::screen(connect);
            if p.id == CONSOLE && screen == Screen::Browser {
                let copied = copied
                    .as_ref()
                    .filter(|(a, _)| conn::authorization(connect).is_some_and(|auth| auth.attempt == *a))
                    .map(|(_, c)| *c);
                return Body::ConsoleSignIn { copied };
            }
            Body::Screen(screen)
        }
    }
}
