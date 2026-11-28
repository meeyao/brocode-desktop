//! One server's link as every surface tells it: connecting, online, its stream reopening, offline
//! (and why), a captive portal or proxy answering in its place, signed out, or a version this app
//! can't use. The app keeps one per server and feeds it what its connection hears (the event
//! stream opening and dropping, with the typed error it dropped with; health probes; the launch's
//! first reads; the OS saying the network changed or the machine woke); the header's status and
//! the reconnect wake come from it, so they can't disagree.
//!
//! Desktop has the stream's `connecting | connected | reconnecting` and a health check apart
//! (`client/src/solid/connection.ts`, `runtime/server/health-machine.ts`); this is Rune's.

use rune::machine::Effects;

use crate::wire_error::{WireError, WireKind};

/// A drop shorter than this says nothing (Kit's grace before the header says Reconnecting).
pub const QUIET: f64 = 2.0;
/// A suspect link whose stream stays up this long is fine after all: the app reopens a stream
/// that's silent for 45 s (`reconnect::STREAM_IDLE`, official `defaultIdleTimeout = 45_000`),
/// and the server writes a heartbeat every 15.
pub const SUSPECT_FOR: f64 = 45.0 + QUIET;

#[derive(Clone, Debug, Default, PartialEq)]
pub enum Link {
    /// Nothing heard yet, or trying again after the network changed.
    #[default]
    Connecting,
    /// The stream is up (`server.connected` heard).
    Online,
    /// The stream dropped at `since` and is reopening; `shown` once it has lasted [`QUIET`].
    Reopening { since: f64, shown: bool },
    /// The stream is up, but a read or write failed for want of a link at `since`; `shown` once
    /// it has lasted [`QUIET`]. Any answer clears it, and so does the stream outliving its idle
    /// limit without dropping ([`SUSPECT_FOR`]).
    Suspect { since: f64, shown: bool },
    /// It took the connection at launch but hasn't answered the first reads since `since`: the
    /// window shows what it saved and says the server is slow, rather than nothing until the
    /// reads time out.
    Slow { since: f64 },
    /// Nothing answers: `why` in the client's words (no connection, no answer in time).
    Offline { since: f64, why: Offline },
    /// Something else answers in its place: a captive portal's sign-in page, a proxy.
    Intercepted { since: f64 },
    /// It refused this app's credentials (401, 403).
    SignedOut,
    /// It runs a version this app can't use.
    Incompatible,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Offline {
    /// Nothing accepts a connection at its address.
    Refused,
    /// It accepted, but didn't answer in time.
    Silent,
}

#[derive(Clone, Debug)]
pub enum Event {
    /// The event stream opened (or reopened).
    StreamUp,
    /// The event stream dropped at `now`, with what it dropped with when the client knows.
    StreamDown { now: f64, error: Option<WireError> },
    /// The launch's first reads found nothing answering.
    Unreachable { now: f64 },
    /// The server took the launch's connection at `now` minus the app's patience and hasn't answered
    /// the first reads.
    FirstReadsSlow { now: f64 },
    /// A health probe: it answers (and its version is usable or not), or refused our credentials.
    Healthy { compatible: bool },
    Unauthorized,
    /// The OS says the network changed or the machine woke: try again now.
    NetworkChanged,
    /// Another call failed on the link itself at `now` (not the server's answer).
    CallFailed { now: f64 },
    /// The server answered something.
    Heard,
    /// After a network change or a wake, the server didn't answer a check at `now`, with what it
    /// failed with: whatever the stream looked like, it's offline now.
    Unanswered { now: f64, error: Option<WireError> },
    Tick { now: f64 },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Effect {
    /// What the header says changed.
    Repaint,
    /// Look again at this time.
    WakeAt(f64),
    /// Reopen the stream and probe now, without waiting out a backoff.
    Reconnect,
}

impl Link {
    pub fn step(&mut self, event: Event) -> Effects<Effect> {
        use Effect::*;
        let before = self.clone();
        let effects = match (&*self, event) {
            // Only the stream clears a stream something else answers: a proxy that mangles the
            // event stream alone (buffering it, a sign-in page for it) leaves the probe answering.
            (_, Event::StreamUp) | (Link::Connecting | Link::Offline { .. } | Link::SignedOut | Link::Incompatible, Event::Healthy { compatible: true }) => {
                *self = Link::Online;
                Effects::none()
            }
            (_, Event::Healthy { compatible: false }) => {
                *self = Link::Incompatible;
                Effects::none()
            }
            (_, Event::Unauthorized) => {
                *self = Link::SignedOut;
                Effects::none()
            }
            (_, Event::StreamDown { now, error }) => match error.as_ref().map(|e| &e.kind) {
                Some(WireKind::Redirected { .. } | WireKind::NotOpenCode { .. }) => {
                    *self = Link::Intercepted { since: now };
                    Effects::none()
                }
                Some(WireKind::Status { status: 401 | 403, .. }) => {
                    *self = Link::SignedOut;
                    Effects::none()
                }
                Some(WireKind::Unreachable(_)) if !matches!(self, Link::Online | Link::Suspect { .. }) => {
                    *self = Link::Offline { since: now, why: Offline::Refused };
                    Effects::none()
                }
                // A suspect link whose stream drops is reopening from now, and keeps what it shows.
                _ if matches!(self, Link::Suspect { .. }) => {
                    let Link::Suspect { shown, .. } = *self else { return Effects::none() };
                    *self = Link::Reopening { since: now, shown };
                    if shown { Effects::none() } else { Effects::one(WakeAt(now + QUIET)) }
                }
                // A drop from online, or one the client can't name: it's reopening, quietly at first.
                _ if matches!(self, Link::Online | Link::Connecting) => {
                    *self = Link::Reopening { since: now, shown: false };
                    Effects::one(WakeAt(now + QUIET))
                }
                Some(WireKind::TimedOut) => {
                    *self = Link::Offline { since: now, why: Offline::Silent };
                    Effects::none()
                }
                _ => Effects::none(),
            },
            (_, Event::Unreachable { now }) => {
                *self = Link::Offline { since: now, why: Offline::Refused };
                Effects::none()
            }
            // Only a link nothing else has spoken for: a stream that opened, or a probe that said
            // otherwise, knows better.
            (Link::Online | Link::Connecting, Event::FirstReadsSlow { now }) => {
                *self = Link::Slow { since: now };
                Effects::none()
            }
            // Said at once, without the quiet grace a drop gets: the check already waited its bound.
            (Link::SignedOut | Link::Incompatible, Event::Unanswered { .. }) => Effects::none(),
            (_, Event::Unanswered { now, error }) => {
                let why = if matches!(error.as_ref().map(|e| &e.kind), Some(WireKind::Unreachable(_))) { Offline::Refused } else { Offline::Silent };
                *self = Link::Offline { since: now, why };
                Effects::none()
            }
            (Link::Offline { .. } | Link::Intercepted { .. } | Link::Reopening { .. } | Link::Suspect { .. } | Link::Slow { .. } | Link::Connecting, Event::NetworkChanged) => {
                *self = Link::Connecting;
                Effects::one(Reconnect)
            }
            (Link::Online, Event::CallFailed { now }) => {
                *self = Link::Suspect { since: now, shown: false };
                Effects::one(WakeAt(now + QUIET))
            }
            (Link::Suspect { .. }, Event::Heard) => {
                *self = Link::Online;
                Effects::none()
            }
            (Link::Suspect { since, shown }, Event::Tick { now }) => {
                let (since, shown) = (*since, *shown);
                if now + 1e-3 >= since + SUSPECT_FOR {
                    *self = Link::Online;
                    return Effects::one(Repaint);
                }
                if shown || now + 1e-3 < since + QUIET {
                    return Effects::one(WakeAt(if shown { since + SUSPECT_FOR } else { since + QUIET }));
                }
                *self = Link::Suspect { since, shown: true };
                Effects::one(WakeAt(since + SUSPECT_FOR))
            }
            (Link::Reopening { since, shown: false }, Event::Tick { now }) => {
                let since = *since;
                if now + 1e-3 < since + QUIET {
                    Effects::one(WakeAt(since + QUIET))
                } else {
                    *self = Link::Reopening { since, shown: true };
                    Effects::none()
                }
            }
            _ => Effects::none(),
        };
        if *self != before {
            let mut e = effects;
            e.extend(std::iter::once(Repaint));
            return e;
        }
        effects
    }

    /// Whether the header says anything: everything but online, connecting and a short drop.
    pub fn shown(&self) -> bool {
        !matches!(self, Link::Online | Link::Connecting | Link::Reopening { shown: false, .. } | Link::Suspect { shown: false, .. })
    }

    /// Whether the header says Reconnecting: the stream reopening, or calls failing, past [`QUIET`].
    pub fn reconnecting(&self) -> bool {
        matches!(self, Link::Reopening { shown: true, .. } | Link::Suspect { shown: true, .. })
    }

    /// Whether it waits on the clock (a grace to run out).
    pub fn ticks(&self) -> bool {
        matches!(self, Link::Reopening { shown: false, .. } | Link::Suspect { .. })
    }

    /// The event stream dropped, or something else answers in the server's place.
    pub fn dropped(&self) -> bool {
        matches!(self, Link::Reopening { .. } | Link::Offline { .. } | Link::Intercepted { .. })
    }
}

