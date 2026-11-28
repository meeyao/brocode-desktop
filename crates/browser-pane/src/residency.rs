//! Explicit per-tab residency state machine (`Active`, `Hidden`, `Frozen`, `Hibernated`, `Waking`).

use rune::time::Instant;

/// Explicit residency phase for a [`crate::BrowserTab`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TabResidency {
    /// Tab is visible in a pane or held awake by an active RPC hold.
    #[default]
    Active,
    /// Tab is hidden (1 FPS, GPU surfaces released) since `since`.
    Hidden { since: Instant },
    /// Tab is frozen via Chromium Page Lifecycle (`Page.setWebLifecycleState(frozen)`) since `since`.
    Frozen { since: Instant },
    /// Tab's CEF browser was torn down via `HibernateTab` since `since` (metadata/downloads preserved).
    Hibernated { since: Instant },
    /// A hibernated tab has sent `OpenTab` and is awaiting engine confirmation.
    Waking,
}

impl TabResidency {
    pub fn is_hibernated(self) -> bool {
        matches!(self, Self::Hibernated { .. })
    }

    pub fn is_frozen(self) -> bool {
        matches!(self, Self::Frozen { .. })
    }

    pub fn is_waking(self) -> bool {
        matches!(self, Self::Waking)
    }

    pub fn hidden_since(self) -> Option<Instant> {
        match self {
            Self::Hidden { since } => Some(since),
            _ => None,
        }
    }

    pub fn inactive_since(self) -> Option<Instant> {
        match self {
            Self::Hidden { since } | Self::Frozen { since } => Some(since),
            _ => None,
        }
    }
}

/// Inputs driving [`step`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResidencyEvent {
    /// Tab became visible in a pane or acquired an active RPC unthrottle hold.
    BecameAwake,
    /// Tab is neither visible nor unthrottled.
    BecameHidden { now: Instant },
    /// Caller requested Tier 2 CDP Freeze (`frozen: true`) or unfreeze (`frozen: false`).
    SetFrozen { frozen: bool, now: Instant },
    /// Caller requested Tier 3 hibernation (`HostMsg::HibernateTab`).
    Hibernate { now: Instant },
    /// Caller started waking a hibernated tab (`HostMsg::OpenTab`).
    BeginWake,
    /// Waking open failed or was refused before engine confirmation.
    WakeFailed { now: Instant },
    /// Engine confirmed the tab via a `TabUpdate`/`TabList` snapshot.
    EngineConfirmed { awake: bool, now: Instant },
    /// Engine reconnected or restarted (`EngineMsg::Ready`).
    EngineReset { has_engine_tab: bool, awake: bool, now: Instant },
    /// Tab was reopened in a new isolation context (`reopen_tab_as_web` / `reopen_tab_as_preview`).
    ContextReopened { has_engine_tab: bool, awake: bool, now: Instant },
}

/// Side effects the caller executes after [`step`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ResidencyEffects {
    /// Send `HostMsg::FreezeTab { tab_id, frozen }` when `Some(frozen)`.
    pub send_freeze: Option<bool>,
    /// True when transitioning into `Hibernated` and the caller should send `HostMsg::HibernateTab`.
    pub send_hibernate: bool,
}

/// Pure transition function for [`TabResidency`].
pub fn step(state: &mut TabResidency, event: ResidencyEvent) -> ResidencyEffects {
    match (*state, event) {
        (TabResidency::Hidden { .. } | TabResidency::Frozen { .. }, ResidencyEvent::BecameAwake) => {
            *state = TabResidency::Active;
            ResidencyEffects::default()
        }
        (TabResidency::Active, ResidencyEvent::BecameHidden { now }) => {
            *state = TabResidency::Hidden { since: now };
            ResidencyEffects::default()
        }
        (TabResidency::Hidden { since }, ResidencyEvent::SetFrozen { frozen: true, .. }) => {
            *state = TabResidency::Frozen { since };
            ResidencyEffects { send_freeze: Some(true), send_hibernate: false }
        }
        (TabResidency::Frozen { .. }, ResidencyEvent::SetFrozen { frozen: false, now }) => {
            *state = TabResidency::Hidden { since: now };
            ResidencyEffects { send_freeze: Some(false), send_hibernate: false }
        }
        // Never allow `Active -> Hibernated`; callers must hide/unhold before hibernating.
        (
            TabResidency::Hidden { .. } | TabResidency::Frozen { .. } | TabResidency::Waking,
            ResidencyEvent::Hibernate { now },
        ) => {
            *state = TabResidency::Hibernated { since: now };
            ResidencyEffects { send_freeze: None, send_hibernate: true }
        }
        (TabResidency::Hibernated { .. }, ResidencyEvent::BeginWake) => {
            *state = TabResidency::Waking;
            ResidencyEffects::default()
        }
        (TabResidency::Waking, ResidencyEvent::WakeFailed { now }) => {
            *state = TabResidency::Hibernated { since: now };
            ResidencyEffects::default()
        }
        (TabResidency::Waking, ResidencyEvent::EngineConfirmed { awake, now }) => {
            *state = if awake {
                TabResidency::Active
            } else {
                TabResidency::Hidden { since: now }
            };
            ResidencyEffects::default()
        }
        // Keep `Waking` intact across `EngineReset` so `flush_pending_opens` +
        // `EngineConfirmed` / `WakeFailed` owns the wake completion transition.
        (
            TabResidency::Frozen { .. },
            ResidencyEvent::EngineReset { has_engine_tab: true, awake, now },
        ) => {
            *state = if awake {
                TabResidency::Active
            } else {
                TabResidency::Hidden { since: now }
            };
            ResidencyEffects::default()
        }
        (_, ResidencyEvent::ContextReopened { has_engine_tab, awake, now }) => {
            *state = if has_engine_tab && awake {
                TabResidency::Active
            } else {
                TabResidency::Hidden { since: now }
            };
            ResidencyEffects::default()
        }
        _ => ResidencyEffects::default(),
    }
}

/// Engine request emitted when an extension targets a discarded (hibernated) tab.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExtensionWakeRequest {
    pub tab_id: u64,
    pub focus: bool,
    pub url: Option<String>,
    pub reload: bool,
}
