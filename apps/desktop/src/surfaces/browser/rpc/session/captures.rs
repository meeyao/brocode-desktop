//! Global idle desktop capture LRU cap across `experimental.browser` v4 sessions (Phase 3b).

use std::collections::BTreeMap;

use opencode_kit::servers::ServerKey;

use super::SessionAttachment;

/// Maximum combined desktop-owned capture bytes across all sessions before oldest idle-session
/// captures are evicted (64 MiB). Active-family and busy sessions are never evicted.
pub(in super::super) const MAX_GLOBAL_IDLE_CAPTURE_BYTES: usize = 64 * 1024 * 1024;

/// Enforces [`MAX_GLOBAL_IDLE_CAPTURE_BYTES`] across all sessions by evicting the oldest
/// captures from idle sessions (sorted by oldest `last_activity`). Never evicts from any
/// protected session (active routed family or busy session/subtree) or any session with
/// active commands or recordings.
pub(in super::super) fn prune_idle_captures(
    sessions: &mut BTreeMap<(ServerKey, String), SessionAttachment>,
    mut is_protected: impl FnMut(&str) -> bool,
) {
    let mut total: usize = sessions
        .values()
        .flat_map(|att| att.captures.values())
        .flat_map(|list| list.iter())
        .map(|c| c.bytes.len())
        .sum();
    if total <= MAX_GLOBAL_IDLE_CAPTURE_BYTES {
        return;
    }

    while total > MAX_GLOBAL_IDLE_CAPTURE_BYTES {
        let victim_key = sessions
            .iter()
            .filter(|((_, sid), att)| {
                att.active_commands.is_empty()
                    && att.trace_recording.is_none()
                    && att.cpu_started.is_empty()
                    && att.heap_chunks.is_empty()
                    && att.captures.values().any(|v| !v.is_empty())
                    && !is_protected(sid)
            })
            .min_by_key(|(_, att)| att.last_activity)
            .map(|(k, _)| k.clone());
        let Some(key) = victim_key else {
            break;
        };
        let Some(att) = sessions.get_mut(&key) else {
            break;
        };
        let Some(evicted) = evict_oldest_capture(att) else {
            break;
        };
        total = total.saturating_sub(evicted);
    }
}

fn evict_oldest_capture(att: &mut SessionAttachment) -> Option<usize> {
    while let Some((tid, cid)) = att.capture_lru.pop_front() {
        if let Some(list) = att.captures.get_mut(&tid)
            && let Some(pos) = list.iter().position(|c| c.id == cid)
        {
            let removed = list.remove(pos);
            if list.is_empty() {
                att.captures.remove(&tid);
            }
            return Some(removed.bytes.len());
        }
    }
    let fallback_tid = att
        .captures
        .iter()
        .filter(|(_, v)| !v.is_empty())
        .min_by_key(|&(&tid, _)| tid)
        .map(|(&tid, _)| tid);
    if let Some(tid) = fallback_tid
        && let Some(list) = att.captures.get_mut(&tid)
        && !list.is_empty()
    {
        let removed = list.remove(0);
        if list.is_empty() {
            att.captures.remove(&tid);
        }
        return Some(removed.bytes.len());
    }
    None
}
