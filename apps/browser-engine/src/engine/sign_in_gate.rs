//! Per-partition readiness gate for the initial remote sign-in cookie copy
//! (`UP-151`, official `packages/gui-plugins/src/browser/profile/link-machine.ts`
//! and `packages/gui-plugins/src/browser/pane.ts:315-319,889-891`).
//!
//! While `share_sign_ins` is enabled on a `ContextKind::RemoteServer` partition, the
//! first profile -> remote cookie copy runs asynchronously so opening or attaching a
//! remote server never stalls. Only HTTP/HTTPS navigations targeting that partition
//! wait until the initial copy finishes, fails, is cancelled, or hits the fail-open
//! timeout; non-HTTP schemes (`about:`, `data:`, etc.) never wait.

use std::collections::HashMap;
use std::time::{Duration, Instant};

/// Fail-open ceiling so a stalled CEF cookie callback can never wedge remote tab loads.
pub(crate) const INITIAL_COPY_TIMEOUT: Duration = Duration::from_secs(10);

/// Returns `true` for `http://` and `https://` URLs (`pane.ts:891` `/^https?:\/\//i`).
/// Non-HTTP schemes (`about:`, `data:`, `blob:`, `chrome-extension:`, `oc-preview:`)
/// return `false` and bypass the sign-in readiness gate.
pub(crate) fn is_http_or_https(url: &str) -> bool {
    let trimmed = url.trim();
    let bytes = trimmed.as_bytes();
    (bytes.len() > 7 && bytes[..7].eq_ignore_ascii_case(b"http://"))
        || (bytes.len() > 8 && bytes[..8].eq_ignore_ascii_case(b"https://"))
}

/// A remote HTTP/HTTPS navigation queued while its partition's initial sign-in copy runs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct HeldNav {
    pub tab_id: u64,
    pub generation: u64,
    pub url: String,
}

/// Explicit per-partition readiness state (`MirrorState` parity).
#[derive(Debug, Default)]
pub(crate) enum GateState {
    /// No initial copy is holding navigations (sharing off, or initial copy already settled).
    #[default]
    Ready,
    /// Initial profile -> remote sign-in copy is in flight (or waiting for a prior wipe to drain).
    Copying {
        deadline: Instant,
        held: Vec<HeldNav>,
    },
}

pub(crate) enum GateEvent {
    /// Initial copy began for this partition; idempotent while already `Copying`.
    CopyStarted { now: Instant },
    /// A navigation arrived for `tab_id` on this partition.
    Navigate(HeldNav),
    /// `tab_id` stopped, closed, hibernated, was suspended, or started a history navigation.
    CancelTab { tab_id: u64 },
    /// Initial copy completed, failed, or sharing was disabled.
    CopySettled,
    /// Periodic check for the fail-open timeout.
    Tick { now: Instant },
}

#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct GateEffects {
    /// `true` when `Navigate` was held by the gate (caller must NOT issue `load_url` yet).
    pub held: bool,
    /// Held navigations released by `CopySettled` or timeout `Tick`.
    pub release: Vec<HeldNav>,
}

/// Pure transition function for a single remote partition's initial sign-in gate.
pub(crate) fn step(state: &mut GateState, event: GateEvent) -> GateEffects {
    match event {
        GateEvent::CopyStarted { now } => {
            if matches!(state, GateState::Ready) {
                *state = GateState::Copying {
                    deadline: now + INITIAL_COPY_TIMEOUT,
                    held: Vec::new(),
                };
            }
            GateEffects::default()
        }
        GateEvent::Navigate(nav) => {
            let GateState::Copying { held, .. } = state else {
                return GateEffects::default();
            };
            if !is_http_or_https(&nav.url) {
                // Non-HTTP navigation never waits and supersedes any earlier held HTTP load.
                held.retain(|item| item.tab_id != nav.tab_id);
                return GateEffects::default();
            }
            if let Some(existing) = held.iter_mut().find(|item| item.tab_id == nav.tab_id) {
                *existing = nav;
            } else {
                held.push(nav);
            }
            GateEffects {
                held: true,
                release: Vec::new(),
            }
        }
        GateEvent::CancelTab { tab_id } => {
            if let GateState::Copying { held, .. } = state {
                held.retain(|item| item.tab_id != tab_id);
            }
            GateEffects::default()
        }
        GateEvent::CopySettled => match std::mem::replace(state, GateState::Ready) {
            GateState::Ready => GateEffects::default(),
            GateState::Copying { held, .. } => GateEffects {
                held: false,
                release: held,
            },
        },
        GateEvent::Tick { now } => {
            let expired = matches!(state, GateState::Copying { deadline, .. } if now >= *deadline);
            if expired {
                step(state, GateEvent::CopySettled)
            } else {
                GateEffects::default()
            }
        }
    }
}

/// Per-partition collection of initial sign-in readiness gates keyed by canonical `server_id`.
#[derive(Default)]
pub(crate) struct SignInGates {
    partitions: HashMap<String, GateState>,
}

impl SignInGates {
    pub(crate) fn begin_copy(&mut self, partition: &str, now: Instant) {
        let state = self.partitions.entry(partition.to_owned()).or_default();
        let _ = step(state, GateEvent::CopyStarted { now });
    }

    /// Returns `true` when `nav` was held behind an in-flight initial sign-in copy.
    pub(crate) fn gate_navigation(&mut self, partition: &str, nav: HeldNav) -> bool {
        let Some(state) = self.partitions.get_mut(partition) else {
            return false;
        };
        step(state, GateEvent::Navigate(nav)).held
    }

    pub(crate) fn settle(&mut self, partition: &str) -> Vec<HeldNav> {
        let Some(state) = self.partitions.get_mut(partition) else {
            return Vec::new();
        };
        step(state, GateEvent::CopySettled).release
    }

    pub(crate) fn settle_all(&mut self) -> Vec<HeldNav> {
        let mut released = Vec::new();
        for state in self.partitions.values_mut() {
            released.extend(step(state, GateEvent::CopySettled).release);
        }
        released
    }

    pub(crate) fn drop_partition(&mut self, partition: &str) {
        self.partitions.remove(partition);
    }

    /// Removes any held navigation for `tab_id`, returning `true` if one was waiting.
    pub(crate) fn cancel_tab(&mut self, tab_id: u64) -> bool {
        let mut removed = false;
        for state in self.partitions.values_mut() {
            if let GateState::Copying { held, .. } = state {
                let before = held.len();
                let _ = step(state, GateEvent::CancelTab { tab_id });
                if let GateState::Copying { held, .. } = state
                    && held.len() != before
                {
                    removed = true;
                }
            }
        }
        removed
    }

    pub(crate) fn is_copying(&self, partition: &str) -> bool {
        matches!(self.partitions.get(partition), Some(GateState::Copying { .. }))
    }

    pub(crate) fn is_tab_held(&self, tab_id: u64) -> bool {
        self.partitions.values().any(|state| {
            matches!(
                state,
                GateState::Copying { held, .. } if held.iter().any(|item| item.tab_id == tab_id)
            )
        })
    }

    /// Advances all copying partitions against `now`; returns timed-out `(partition, released)`.
    pub(crate) fn tick(&mut self, now: Instant) -> Vec<(String, Vec<HeldNav>)> {
        let mut out = Vec::new();
        for (partition, state) in &mut self.partitions {
            let was_copying = matches!(state, GateState::Copying { .. });
            let eff = step(state, GateEvent::Tick { now });
            if was_copying && matches!(state, GateState::Ready) {
                out.push((partition.clone(), eff.release));
            }
        }
        out
    }
}
