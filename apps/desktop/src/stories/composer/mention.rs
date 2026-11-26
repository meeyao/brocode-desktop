//! The storybook's Mentions story (Composer › Mentions): a sent prompt whose bubble shows a badge
//! for each kind of mention (agent, skill, file, folder, reference, session), and a draft in the
//! composer with the same badges. P lands the prompt again, R replays. Also Timeline › Session
//! mention.

use crate::stories::fixture_ids::SIXTH as OTHER;
use rune::{Play, TextEdit, Tx};
use serde_json::json;

use crate::App;
use crate::stories::{Trigger, emit, replay, sid};

pub const KEYS: &[Trigger] = &[replay(open), Trigger { key: 'p', label: "The prompt lands", run: |s, tx| prompt(s, tx) }];


fn open(s: &mut App, tx: &mut Play<App>) {
    s.finish_stream();
    *s.composer = draft();
    s.composer.focused = true;
    prompt(s, tx);
}

/// "Ask @explore and @opencode about @src/stream.rs in @src/ui/, like @<session> did".
pub(crate) fn draft() -> TextEdit {
    let mut e = TextEdit::new("");
    e.insert("Ask ");
    e.insert_pill("agent:explore", "@explore");
    e.insert(" and ");
    e.insert_pill("skill:opencode", "@opencode");
    e.insert(" about ");
    e.insert_pill("file:src/stream.rs", "@src/stream.rs");
    e.insert(" in ");
    e.insert_pill("file:src/ui/", "@src/ui/");
    e.insert(", like ");
    e.insert_pill(format!("session:{OTHER}"), "@Migrate config loader to the new schema");
    e.insert(" did");
    e
}

/// The sent prompt: its text and the mentions the server records for it.
fn prompt(s: &mut App, tx: &mut Tx) {
    let text = format!("Have @explore use @opencode on @src/stream.rs, @src/ui/ and @docs, as in @session:{OTHER}");
    let at = |m: &str| {
        let start = text.find(m).unwrap_or(0);
        json!({ "start": start, "end": start + m.len(), "text": m })
    };
    let n = s.transcripts.get(&sid(s)).map_or(0, |t| t.messages.len());
    emit(
        s,
        "session.prompted",
        json!({
            "messageID": format!("msg_zzzzmention{n:04}"),
            "text": text,
            "agents": [{ "name": "explore", "mention": at("@explore") }],
            "skills": [{ "id": "opencode", "name": "Brocode", "mention": at("@opencode") }],
            "files": [
                { "uri": "file:///repo/src/stream.rs", "name": "stream.rs", "mime": "text/plain", "mention": at("@src/stream.rs") },
                { "uri": "file:///repo/src/ui", "name": "ui", "mime": "text/plain", "mention": at("@src/ui/") },
                { "uri": "file:///repo/docs", "name": "docs", "mime": "application/x-directory", "mention": at("@docs") },
            ],
        }),
        tx,
    );
}

/// Timeline › Session mention (p279): Kit's prompt naming a known session and one this window
/// hasn't loaded, with the stale offsets a local echo once left (`the @WORK …`, five bytes at
/// 4). Each reads as one badge, its title or short id; pressing one opens that session.
pub const SESSION_KEYS: &[Trigger] = &[replay(open_sessions)];

fn open_sessions(s: &mut App, tx: &mut Play<App>) {
    s.finish_stream();
    let n = s.transcripts.get(&sid(s)).map_or(0, |t| t.messages.len());
    let id = format!("msg_zzzzsesmention{n:04}");
    let text = format!("the @session:{OTHER} session, and @session:ses_fx999900aaaaaaaaaaaaaaaaaa too");
    opencode_kit::mention::set_user_mentions(&id, vec![opencode_kit::mention::UserMention { range: 4..9, kind: opencode_kit::mention::MentionKind::Session }]);
    emit(s, "session.prompted", json!({ "messageID": id, "text": text }), tx);
}

rune_register::register!(crate::stories::STORY_LIST: crate::stories::Story = [
    crate::stories::Story::session("Composer/Mentions", KEYS),
    crate::stories::Story::session("Timeline/Session mention", SESSION_KEYS),
]);
