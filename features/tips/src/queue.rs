//! The tips' pacing (`packages/app/src/tips/queue.ts` `selectTip`) and the machine that shows
//! them (`tips/model.ts` `createTipToasts`, with the coordinator's `tip` from
//! `shell/notifications/coordinator.ts`): nothing for [`STARTUP_DELAY`] after the window opens,
//! the first tip [`FIRST_TIP_DELAY_MS`] after the app was first seen, then at most one a day, each
//! one until it's closed or its button is pressed, shown by one focused window at a time. Checked
//! when the person next acts (input, focus, a route change), never on an idle timer.

use std::cell::Cell;

use serde::{Deserialize, Serialize};

/// `queue.ts` `TIP_INTERVAL_MS`.
pub const TIP_INTERVAL_MS: i64 = 24 * 60 * 60 * 1000;
/// `queue.ts` `FIRST_TIP_DELAY_MS`.
pub const FIRST_TIP_DELAY_MS: i64 = 60 * 60 * 1000;
/// `model.ts` `STARTUP_DELAY`, in the clock's seconds.
pub const STARTUP_DELAY: f64 = 2.0 * 60.0;

/// One of `model.ts`'s tips, in its order, which is their priority.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tip {
    Browse,
    Hex,
}

pub const TIPS: [Tip; 2] = [Tip::Browse, Tip::Hex];

impl Tip {
    pub fn id(self) -> &'static str {
        match self {
            Tip::Browse => "browse",
            Tip::Hex => "hex",
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Tip::Browse => opencode_i18n::t!("tips.browse.title"),
            Tip::Hex => opencode_i18n::t!("tips.hex.title"),
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Tip::Browse => opencode_i18n::t!("tips.browse.description"),
            Tip::Hex => opencode_i18n::t!("tips.hex.description"),
        }
    }

    pub fn install(self) -> &'static str {
        match self {
            Tip::Browse => opencode_i18n::t!("tips.browse.install"),
            Tip::Hex => opencode_i18n::t!("tips.hex.install"),
        }
    }

    /// What its button opens.
    pub fn url(self) -> &'static str {
        match self {
            Tip::Browse => "https://chromewebstore.google.com/detail/mfnicocicmmlkpjnaffgihfjhdgjkdjg?utm_source=brocode-desktop&utm_campaign=tip-browse",
            // anoma.ly/hex 404s with a query string; the trailing slash is required.
            Tip::Hex => "https://anoma.ly/hex/?utm_source=brocode-desktop&utm_campaign=tip-hex",
        }
    }

    /// Only offer an install while the corresponding tool is missing.
    /// Matches `packages/app/src/tips/model.ts` (`#64583` suppresses Hex on `(hover: none)` devices).
    fn applies(self, installed: &Installed) -> bool {
        match self {
            Tip::Browse => !installed.browse,
            Tip::Hex => !installed.touch && installed.hex == Some(false),
        }
    }
}

/// `model.ts` `History`, kept in `opencode.global.dat`'s `tips.queue` as Desktop keeps it.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct History {
    pub current: String,
    pub shown_at: i64,
    pub dismissed: Vec<String>,
    pub dismissed_at: i64,
    pub first_seen: i64,
}

impl History {
    fn last(&self) -> i64 {
        self.shown_at.max(self.dismissed_at)
    }

    /// Handled: closed, or its button pressed.
    fn handled(&self, tip: Tip, now: i64) -> History {
        let mut dismissed = self.dismissed.clone();
        if !dismissed.iter().any(|d| d == tip.id()) {
            dismissed.push(tip.id().to_owned());
        }
        History { current: String::new(), dismissed, dismissed_at: now, ..self.clone() }
    }
}

/// `selectTip`: the tip shown last while it still applies, else the first that does once its
/// time has come.
pub fn select(history: &History, now: i64, installed: &Installed) -> Option<Tip> {
    let eligible = |tip: &Tip| !history.dismissed.iter().any(|d| d == tip.id()) && tip.applies(installed);
    if let Some(current) = TIPS.iter().find(|t| t.id() == history.current && eligible(t)) {
        return Some(*current);
    }
    let last = history.last();
    if last != 0 && now < last + TIP_INTERVAL_MS {
        return None;
    }
    let first_seen = if history.first_seen == 0 { now } else { history.first_seen };
    if last == 0 && now < first_seen + FIRST_TIP_DELAY_MS {
        return None;
    }
    TIPS.iter().copied().find(|t| eligible(t))
}

thread_local! {
    /// The coordinator's `opencode:tip` lock: the window showing a tip, so no other one does.
    static HELD: Cell<bool> = const { Cell::new(false) };
}

/// This window's hold on the one tip shown at a time; let go when dropped, as the Web Lock is
/// when its window's toast goes or the window closes.
#[derive(Debug)]
pub struct Claim(());

impl Claim {
    /// `locks.request("opencode:tip", { ifAvailable: true })`.
    pub fn take() -> Option<Claim> {
        (!HELD.replace(true)).then_some(Claim(()))
    }
}

impl Drop for Claim {
    fn drop(&mut self) {
        HELD.set(false);
    }
}

/// Whether the server's MCP list was asked for since its link last came up (`model.ts` syncs it
/// each time the connection comes back).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Sync {
    #[default]
    Offline,
    Asked,
}

#[derive(Debug, Default)]
pub enum Phase {
    #[default]
    Idle,
    /// User activity asked whether a tip is due; waits only while `browse` or `hex` is still unknown.
    Checking,
    /// This window shows `tip` in toast `toast`, holding the claim until it leaves this phase.
    Showing { tip: Tip, toast: u64, _claim: Claim },
}

#[derive(Debug, Default)]
pub enum State {
    /// The window hasn't polled yet.
    #[default]
    Unstarted,
    /// Just opened: nothing until the clock reaches `until` and the person acts.
    Starting { until: f64 },
    Awake { sync: Sync, phase: Phase },
}

/// What a poll reads.
pub struct Poll {
    /// The app's clock, in seconds.
    pub clock: f64,
    /// The wall time, in epoch milliseconds.
    pub now: i64,
    pub history: History,
    /// Whether the server's MCP list has a `browse` server; `None` until it's listed.
    pub browse_installed: Option<bool>,
    /// The launch's local Hex check; unknown until the worker finishes.
    pub hex_installed: Option<bool>,
    /// True on touch-only (`hover: none`) devices where the desktop Hex tip does not apply (#64583).
    pub touch: bool,
    pub online: bool,
    /// The window is focused and shown (`document.hasFocus() && !document.hidden`).
    pub open: bool,
    /// Input, window focus or a route change since the last poll: when due tips are checked, so an
    /// untouched window arms no timer.
    pub acted: bool,
}

pub struct Installed {
    pub browse: bool,
    pub hex: Option<bool>,
    pub touch: bool,
}

pub enum Event {
    Polled(Poll),
    /// The toast of [`Effect::Show`] is up.
    Opened { tip: Tip, toast: u64, claim: Claim },
    /// Toast `toast` was closed by the person, or its button pressed (`onDismiss`, the action).
    Handled { toast: u64, history: History, now: i64 },
    /// A scene shows `tip` at once, outside the pacing.
    Forced(Tip),
}

#[derive(Debug, PartialEq)]
pub enum Effect {
    SyncMcp,
    CheckHex,
    /// Shows `tip` if no other window holds the [`Claim`], then writes `history` when given.
    Show { tip: Tip, history: Option<History> },
    Hide(u64),
    Write(History),
}

pub fn step(state: &mut State, event: Event) -> Vec<Effect> {
    match event {
        Event::Polled(poll) => polled(state, poll),
        Event::Opened { tip, toast, claim } => {
            let sync = match state {
                State::Awake { sync, .. } => *sync,
                _ => Sync::Offline,
            };
            *state = State::Awake { sync, phase: Phase::Showing { tip, toast, _claim: claim } };
            vec![]
        }
        Event::Handled { toast, history, now } => {
            let State::Awake { phase, .. } = state else { return vec![] };
            let Phase::Showing { tip, toast: shown, .. } = phase else { return vec![] };
            if *shown != toast {
                return vec![];
            }
            let effects = vec![Effect::Write(history.handled(*tip, now))];
            *phase = Phase::Idle;
            effects
        }
        Event::Forced(tip) => vec![Effect::Show { tip, history: None }],
    }
}

fn polled(state: &mut State, poll: Poll) -> Vec<Effect> {
    match state {
        State::Unstarted => {
            *state = State::Starting { until: poll.clock + STARTUP_DELAY };
            vec![]
        }
        State::Starting { until } if poll.clock + 1e-3 < *until || !poll.acted => vec![],
        State::Starting { .. } => {
            // Past the startup quiet window, checked on the person's next input, focus or route
            // change instead of a timer. Browse can show once listed; Hex waits for an explicit false
            // and is skipped on touch-only devices (#64583).
            let check_hex = !poll.touch && poll.hex_installed.is_none();
            *state = State::Awake { sync: Sync::Offline, phase: Phase::Checking };
            let mut effects = polled(state, poll);
            if check_hex {
                effects.push(Effect::CheckHex);
            }
            effects
        }
        State::Awake { sync, phase } => {
            let mut effects = Vec::new();
            if !poll.online {
                *sync = Sync::Offline;
            }
            if poll.acted && matches!(phase, Phase::Idle) {
                *phase = Phase::Checking;
            }
            if matches!(phase, Phase::Idle) {
                return effects;
            }
            if *sync == Sync::Offline && poll.online {
                *sync = Sync::Asked;
                effects.push(Effect::SyncMcp);
            }
            let Some(browse) = poll.browse_installed else { return effects };
            let selected = select(&poll.history, poll.now, &Installed { browse, hex: poll.hex_installed, touch: poll.touch });
            if let Phase::Showing { tip, toast, .. } = phase {
                if selected == Some(*tip) {
                    return effects;
                }
                effects.push(Effect::Hide(*toast));
                *phase = Phase::Idle;
                return effects;
            }
            if browse && !poll.touch && poll.hex_installed.is_none() {
                return effects;
            }
            *phase = Phase::Idle;
            let Some(tip) = selected.filter(|_| poll.open) else { return effects };
            // `model.ts`'s show callback: the first showing of a tip starts its day.
            let history = (poll.history.current != tip.id()).then(|| History { current: tip.id().to_owned(), shown_at: poll.now, ..poll.history });
            effects.push(Effect::Show { tip, history });
            effects
        }
    }
}
