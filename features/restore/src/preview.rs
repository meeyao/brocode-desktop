//! The shown session's first screen, kept in the app's database so the next launch paints it
//! from the boot packet before any server answers (`opencode_kit::model::preview`, a compact
//! binary form decoded without parsing JSON). It's the session's last two turns (what a window
//! opened at the bottom shows, and one scroll step up), at most [`BYTES`]; read up from its end,
//! it's the turn of the place the window keeps ([`crate::places`]) and what follows it, up to
//! [`PLACE_BYTES`], so the next launch draws the place in its first frame. When that stops short
//! of the end, the latest page lands under it with a gap between, which history fills first
//! (`session_pages`: `State::gap`).
//!
//! It's written when it would change what a launch shows: when the shown session settles (its
//! reply finished, a message came or went), when the window's place moves to another turn and
//! holds still ([`preview_place`], as the place is written), and, streaming or not, as the
//! window hides, closes or quits. A streamed token or a scroll step never writes it.

use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::Arc;

use opencode_kit::model::{Message, Transcript, preview};
use opencode_kit::servers::ServerKey;

use crate::{RestoreHost, RestoreRoute};

/// A preview stays inside the boot record's first read (64 KiB pages).
const BYTES: usize = 48 * 1024;
/// A place's preview: its turn and a few screens under it, not the whole way to the end.
const PLACE_BYTES: usize = 12 * 1024;

/// What each session's preview was last written (or read) from: its messages' stamp, and the
/// turn the place it starts from stood in.
#[derive(Default)]
pub struct Previews(HashMap<String, (u64, Option<usize>)>);

/// What a session's preview depends on, cheap to take every poll.
pub fn stamp(messages: &[Message]) -> u64 {
    let mut h = DefaultHasher::new();
    messages.len().hash(&mut h);
    if let Some(last) = messages.last() {
        last.id().hash(&mut h);
        match last {
            Message::Assistant(a) => (a.completed, a.parts.len(), a.error.is_some()).hash(&mut h),
            Message::Note { failed, step, .. } => (failed, step.completed).hash(&mut h),
            _ => {}
        }
    }
    h.finish()
}

/// The first message of the turn message `at` is in.
fn turn_of(messages: &[Message], at: usize) -> usize {
    if messages.is_empty() {
        return 0;
    }
    messages[..=at.min(messages.len() - 1)].iter().rposition(|m| matches!(m, Message::User { .. })).unwrap_or(0)
}

/// The preview's bytes: from the start of the turn before the last, trimmed from the front to fit.
/// From `turn`, the place's, instead when that's further up: its turn and the messages after it
/// up to [`PLACE_BYTES`] (the place's turn whole if it alone is more, up to [`BYTES`]).
pub fn encode(messages: &[Message], turn: Option<usize>) -> Vec<u8> {
    let users: Vec<usize> = messages.iter().enumerate().filter(|(_, m)| matches!(m, Message::User { .. })).map(|(i, _)| i).collect();
    let start = users.len().checked_sub(2).map_or(0, |i| users[i]);
    if let Some(place) = turn.filter(|&t| t < start) {
        // From the turn before the place's, so the place isn't the transcript's first row, which
        // the history's loading look above it would move. The place's turn ends at the next
        // prompt.
        let turn = users.iter().copied().rev().find(|&u| u < place).unwrap_or(place);
        let own = users.iter().copied().find(|&u| u > place).unwrap_or(messages.len());
        if let Some(bytes) = preview::encode_prefix_within(&messages[turn..], own - turn, PLACE_BYTES, BYTES) {
            return bytes;
        }
    }
    preview::encode_suffix_within(&messages[start..], BYTES)
}

/// The boot packet's preview of the window's shown session, decoded into its transcript: the
/// first frame draws it while the server connects.
pub(crate) fn take_boot_preview(host: &mut dyn RestoreHost, server: &ServerKey, id: &str) -> bool {
    let Some((_, bytes)) = host.storage_boot_preview(server.as_str(), id) else { return false };
    let Some(messages) = preview::decode(&bytes) else { return false };
    if messages.is_empty() {
        return false;
    }
    rune::marks::mark("state.preview");
    host.restore_mut().previews.0.insert(id.to_owned(), (stamp(&messages), None));
    host.insert_transcript(id.to_owned(), Transcript { session: id.to_owned(), messages });
    true
}

/// The turn the shown session's place stands in, when it's read up from its end.
fn place_turn(host: &dyn RestoreHost, id: &str, messages: &[Message]) -> Option<usize> {
    let (row, _) = host.timeline_place(id)?;
    host.timeline_row_message(id, &row).map(|at| turn_of(messages, at))
}

/// The sessions whose preview starts after their first message (`{ <session>: true }`), in the
/// boot record with the previews: [`older`].
const OLDER: &str = "rune.preview.older";

/// Whether session `id` has messages before its preview's first, as the run that wrote the
/// preview saw it. A profile from before this was kept says yes: a session someone comes back to
/// is seldom two turns long.
pub(crate) fn older(host: &dyn RestoreHost, id: &str) -> bool {
    host.storage_json(crate::GLOBAL, OLDER).is_none_or(|v| v[id].as_bool() == Some(true))
}

/// Writes the shown session's preview if what it would hold (`msg_stamp`, from `turn`) isn't
/// what it holds.
fn write_preview(host: &mut dyn RestoreHost, server: &str, id: &str, msg_stamp: u64, turn: Option<usize>) {
    if host.restore().previews.0.get(id) == Some(&(msg_stamp, turn)) {
        return;
    }
    let (bytes, older): (Arc<[u8]>, bool) = {
        let Some(t) = host.transcript(id) else { return };
        let bytes = encode(&t.messages, turn);
        // Before the preview's first message: what the transcript holds, or what its server still does.
        let older = preview::count(&bytes).is_none_or(|n| n < t.messages.len()) || host.more_above(id);
        (Arc::from(bytes), older)
    };
    let updated = host.session_updated(id).unwrap_or(0);
    host.storage_set_preview(server, id, Some((updated, bytes)));
    host.restore_mut().previews.0.insert(id.to_owned(), (msg_stamp, turn));
    // Kept for the sessions that have a tab: a closed one's preview goes with it.
    let tabs = host.session_tab_ids();
    host.storage_update_json(crate::GLOBAL, OLDER, &mut |map| {
        map.retain(|session, _| session == id || tabs.contains(session));
        if older {
            map.insert(id.to_owned(), true.into());
        } else {
            map.remove(id);
        }
    });
}

/// The shown session's place moved and held still (`places`): its preview runs from the
/// place's turn now. Not while streaming, and not while the transcript is still the boot
/// record's.
pub fn preview_place(host: &mut dyn RestoreHost) {
    let RestoreRoute::Session { server, id } = host.route() else { return };
    let (msg_stamp, turn) = {
        let Some(t) = host.transcript(&id) else { return };
        let streaming = matches!(t.messages.last(), Some(Message::Assistant(a)) if a.completed.is_none());
        // Nor while history fills the gap under a place's preview: the transcript has a hole.
        if streaming || t.messages.is_empty() || host.is_cold_transcript(&id) || host.has_pages_gap(&id) {
            return;
        }
        (stamp(&t.messages), place_turn(host, &id, &t.messages))
    };
    write_preview(host, server.as_str(), &id, msg_stamp, turn);
}

/// Keeps the shown session's preview current: when it settles, or (`now`) whatever it's doing,
/// as the window hides, closes or quits.
pub fn poll_preview(host: &mut dyn RestoreHost, now: bool) {
    let RestoreRoute::Session { server, id } = host.route() else { return };
    let (msg_stamp, turn, copy) = {
        let Some(t) = host.transcript(&id) else { return };
        let streaming = matches!(t.messages.last(), Some(Message::Assistant(a)) if a.completed.is_none());
        // A transcript with a gap under a place's preview keeps the preview and copy it came from.
        if (streaming && !now) || t.messages.is_empty() || host.has_pages_gap(&id) {
            return;
        }
        let msg_stamp = stamp(&t.messages);
        // The session's copy for reading it later (`copies`), once it has settled.
        // A transcript that's still the boot record's two turns would overwrite a fuller copy.
        let cold = host.is_cold_transcript(&id);
        let copy = (!streaming && !cold && crate::copies::copy_due(host, &id, msg_stamp)).then(|| {
            rune::marks::mark("state.copy.encode");
            let bytes = crate::copies::encode(&t.messages);
            rune::marks::mark("state.copy.encoded");
            bytes
        });
        // The place is read as the messages change and as the window hides, closes or quits; as
        // it moves, `preview_place` keeps up.
        let kept = host.restore().previews.0.get(&id).copied();
        let turn = if now || kept.is_none_or(|(s, _)| s != msg_stamp) {
            place_turn(host, &id, &t.messages)
        } else {
            kept.and_then(|(_, turn)| turn)
        };
        (msg_stamp, turn, copy)
    };
    write_preview(host, server.as_str(), &id, msg_stamp, turn);
    if let Some(bytes) = copy {
        crate::copies::store_copy(host, &server, &id, msg_stamp, bytes);
    }
}
