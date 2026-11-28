//! Delete: Desktop's confirmation (`session/delete-machine.ts`, `home/sessions/controller.tsx:
//! 423-524`) stays, since once the server has deleted a session its copy is gone for good. Beyond
//! Desktop, which sends the delete at once and waits with both buttons off, Delete in the dialog
//! closes it and takes the session and its subagents off the list at once, and a toast offers it
//! back for its 8 s: only when the toast goes is the delete sent (at once when the app quits).
//! Undo puts the sessions back, nothing sent. Once sent, the server decides: they go with their
//! tabs when it has deleted them, and come back with Desktop's failure toast if it refuses or
//! hasn't answered in 15 s. Killed before the toast goes, nothing was sent: the session is still
//! there next launch. The session is on its own server, whichever one the window works against
//! (`ProjectSessionRow`'s `ctx()`).

use opencode_kit::client::Write;
use opencode_kit::model::SessionInfo;
use opencode_kit::servers::ServerKey;
use rune::Tx;
use rune::machine::Effects;

use crate::DELETE_OVERLAY_ID;
use crate::host::{SessionMenuHost, on_server};

/// `SESSION_DELETE_TIMEOUT_MS`.
pub const TIMEOUT: f64 = 15.0;

/// The session Delete asks about: on its server, with the title the dialog names it by
/// (`sessionLabel(props.session)`, taken as it opens).
#[derive(Clone, Debug, PartialEq)]
pub struct Asked {
    pub session: String,
    pub server: ServerKey,
    pub title: String,
}

/// The confirmation.
#[derive(Clone, Debug, Default)]
pub enum Delete {
    #[default]
    Closed,
    Confirming { asked: Asked },
}

pub enum Event {
    Ask(Asked),
    /// Cancel, Escape or a press outside (`cancel.requested`).
    Cancel,
    /// Delete (`confirm.requested`): `tree` is the session and its subagents, empty once it's gone.
    Confirm { tree: Vec<SessionInfo> },
}

#[derive(Debug)]
pub enum Effect {
    /// Take these sessions off `server`'s list (the data layer's hide).
    Hide(ServerKey, Vec<String>),
    /// Offer `session` back; the delete is sent when the offer's toast goes.
    Defer { server: ServerKey, session: String, hidden: Vec<SessionInfo> },
}

pub fn step(delete: &mut Delete, event: Event) -> Effects<Effect> {
    match (std::mem::take(delete), event) {
        (_, Event::Ask(asked)) => {
            *delete = Delete::Confirming { asked };
            Effects::none()
        }
        (Delete::Confirming { .. }, Event::Cancel) => Effects::none(),
        (Delete::Confirming { asked }, Event::Confirm { tree }) => {
            if tree.is_empty() {
                return Effects::none();
            }
            let hide = Effect::Hide(asked.server.clone(), tree.iter().map(|s| s.id.clone()).collect());
            Effects::two(hide, Effect::Defer { server: asked.server, session: asked.session, hidden: tree })
        }
        (state, _) => {
            *delete = state;
            Effects::none()
        }
    }
}

/// A delete sent, its sessions hidden until the server answers.
#[derive(Clone, Debug)]
struct Sending {
    server: ServerKey,
    session: String,
    hidden: Vec<SessionInfo>,
    due: f64,
}

/// The deletes sent and not answered yet.
#[derive(Clone, Debug, Default)]
pub struct Sent(Vec<Sending>);

pub enum SentEvent {
    /// The toast went: the delete of `session` on `server` goes now.
    Sent { server: ServerKey, session: String, hidden: Vec<SessionInfo>, now: f64 },
    /// `server` answered the delete of `session`.
    Replied { session: String, server: ServerKey, ok: bool },
    Tick { now: f64 },
}

#[derive(Debug)]
pub enum SentEffect {
    /// `server` deleted them: their tabs and page go (`session.removed`).
    Removed(ServerKey, Vec<String>),
    /// Put hidden sessions back on `server`'s list, and say it failed.
    Failed(ServerKey, Vec<SessionInfo>),
    WakeAt(f64),
}

impl Sent {
    pub fn step(&mut self, event: SentEvent) -> Effects<SentEffect> {
        match event {
            SentEvent::Sent { server, session, hidden, now } => {
                let due = now + TIMEOUT;
                self.0.retain(|s| !(s.session == session && s.server == server));
                self.0.push(Sending { server, session, hidden, due });
                Effects::one(SentEffect::WakeAt(due))
            }
            SentEvent::Replied { session, server, ok } => {
                let Some(at) = self.0.iter().position(|s| s.session == session && s.server == server) else {
                    // A delete that timed out can still land: the session is gone, so its tabs go too.
                    return if ok { Effects::one(SentEffect::Removed(server, vec![session])) } else { Effects::none() };
                };
                let s = self.0.remove(at);
                if ok {
                    return Effects::one(SentEffect::Removed(s.server, s.hidden.into_iter().map(|h| h.id).collect()));
                }
                Effects::one(SentEffect::Failed(s.server, s.hidden))
            }
            SentEvent::Tick { now } => {
                let (late, waiting): (Vec<Sending>, Vec<Sending>) = std::mem::take(&mut self.0).into_iter().partition(|s| now + 1e-9 >= s.due);
                self.0 = waiting;
                let mut effects: Vec<SentEffect> = late.into_iter().map(|s| SentEffect::Failed(s.server, s.hidden)).collect();
                effects.extend(self.next_due().map(SentEffect::WakeAt));
                Effects::Many(effects)
            }
        }
    }

    /// When the oldest delete in flight times out.
    pub fn next_due(&self) -> Option<f64> {
        self.0.iter().map(|s| s.due).reduce(f64::min)
    }

    /// Whether no deletes are currently waiting for a server reply.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// Asks before deleting `id` on the server the window works against.
pub fn ask_delete(host: &mut dyn SessionMenuHost, id: &str) {
    let server = host.active_server();
    ask_delete_on(host, &server, id);
}

/// Asks before deleting `id` on `server`, named by its title there.
pub fn ask_delete_on(host: &mut dyn SessionMenuHost, server: &ServerKey, id: &str) {
    let title = on_server(host, server, |h| h.session(id).map(crate::title_of))
        .flatten()
        .unwrap_or_else(|| opencode_i18n::t!("command.session.new").to_owned());
    let asked = Asked { session: id.to_owned(), server: server.clone(), title };
    dispatch(host, Event::Ask(asked), &mut Tx::default());
}

/// Cancel, Escape or a press outside Delete's confirmation.
pub fn cancel_delete(host: &mut dyn SessionMenuHost, tx: &mut Tx) {
    dispatch(host, Event::Cancel, tx);
}

/// Delete: the session the confirmation asks about and its subagents leave its server's
/// list, and the toast that offers them back sends the delete when it goes (`delete`).
pub fn delete_session(host: &mut dyn SessionMenuHost, tx: &mut Tx) {
    let Some(asked) = host.menus().asked().cloned() else {
        return;
    };
    let tree = on_server(host, &asked.server, |h| {
        let mut gone = vec![asked.session.clone()];
        let mut at = 0;
        while at < gone.len() {
            let parent = gone[at].clone();
            gone.extend(h.sessions().iter().filter(|s| s.parent.as_deref() == Some(&parent)).map(|s| s.id.clone()));
            at += 1;
        }
        gone.iter().filter_map(|g| h.session(g).cloned()).collect()
    })
    .unwrap_or_default();
    dispatch(host, Event::Confirm { tree }, tx);
}

/// The deadline of the deletes sent.
pub fn poll_delete(host: &mut dyn SessionMenuHost, now: f64, tx: &mut Tx) -> bool {
    // Only a delete in flight hears the clock: polls leave the menus untouched otherwise, so
    // regions that read them stay reused.
    let Some(due) = host.menus().sent.next_due() else {
        return false;
    };
    if now + 1e-9 < due {
        tx.wake_at(due);
        return false;
    }
    sent_event(host, SentEvent::Tick { now }, "", tx);
    true
}

/// Runs `event` through Delete's confirmation and carries out what it asks.
fn dispatch(host: &mut dyn SessionMenuHost, event: Event, tx: &mut Tx) {
    let was_open = host.menus().asked().is_some();
    if !was_open && matches!(event, Event::Cancel | Event::Confirm { .. }) {
        return;
    }
    let effects = step(&mut host.menus_mut().delete, event);
    let now_open = host.menus().asked().is_some();
    match (was_open, now_open) {
        (false, true) => {
            tx.instant();
            host.on_overlay_opened(DELETE_OVERLAY_ID, None);
        }
        (true, false) => {
            tx.instant();
            host.on_overlay_closed(DELETE_OVERLAY_ID);
        }
        _ => {}
    }
    for effect in effects {
        tx.instant();
        match effect {
            Effect::Hide(server, ids) => {
                let _ = on_server(host, &server, |h| {
                    h.record_inbox_shown();
                    if let Some(sessions) = h.sessions_mut() {
                        sessions.retain(|s| !ids.contains(&s.id));
                    }
                });
            }
            Effect::Defer { server, session, hidden } => {
                host.offer_delete_undo(opencode_i18n::t!("rune.undo.sessionDeleted"), server, session, hidden);
            }
        }
    }
}

/// The delete of `session` on `server` goes now: its undo toast went, or the app quits.
pub fn send_delete(host: &mut dyn SessionMenuHost, server: ServerKey, session: String, hidden: Vec<SessionInfo>, tx: &mut Tx) {
    let now = host.clock();
    sent_event(host, SentEvent::Sent { server: server.clone(), session: session.clone(), hidden, now }, "", tx);
    // A story's write goes nowhere and a build that can't write sends none: each answers at once.
    let sent = on_server(host, &server, |h| h.write_server(Write::Delete(session.clone()))).unwrap_or(false);
    if host.stage_active() || !sent {
        sent_event(host, SentEvent::Replied { session, server, ok: sent }, "", tx);
    }
}

/// Undo on a delete's toast: its sessions are listed again, nothing sent.
pub fn undelete(host: &mut dyn SessionMenuHost, server: &ServerKey, hidden: Vec<SessionInfo>) {
    let _ = on_server(host, server, |h| unhide(h, hidden));
}

fn unhide(host: &mut dyn SessionMenuHost, hidden: Vec<SessionInfo>) {
    if hidden.is_empty() {
        return;
    }
    let missing: Vec<SessionInfo> = hidden.into_iter().filter(|h| host.session(&h.id).is_none()).collect();
    if !missing.is_empty()
        && let Some(sessions) = host.sessions_mut()
    {
        sessions.extend(missing);
    }
}

/// Runs `event` through the deletes sent and carries out what it asks.
pub(crate) fn sent_event(host: &mut dyn SessionMenuHost, event: SentEvent, why: &str, tx: &mut Tx) {
    let effects = host.menus_mut().sent.step(event);
    for effect in effects {
        match effect {
            SentEffect::WakeAt(due) => tx.wake_at(due),
            SentEffect::Removed(server, gone) => {
                tx.instant();
                let _ = on_server(host, &server, |h| {
                    h.record_inbox_shown();
                    if let Some(sessions) = h.sessions_mut() {
                        sessions.retain(|s| !gone.contains(&s.id));
                    }
                    h.on_sessions_removed(&server, &gone, tx);
                });
            }
            SentEffect::Failed(server, hidden) => {
                tx.instant();
                let _ = on_server(host, &server, |h| unhide(h, hidden));
                // A refusal with the server's words has its own toast (`from_server`, in
                // `session_write_replied`).
                if why.is_empty() {
                    host.toast_fail(opencode_i18n::t!("session.delete.failed.title"), "");
                }
            }
        }
    }
}
