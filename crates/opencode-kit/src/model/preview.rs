//! A session's first screen in a compact binary form: the messages the timeline draws when the
//! session opens, as the app parsed them, with what its user messages keep beside them (files,
//! path cards, comments, mentions, when each was sent). The app's database keeps one per open
//! session and hands the shown one to the first frame, so a restored session paints its
//! messages before any server answers, with no JSON to parse: decoding is a walk over
//! length-prefixed fields.
//!
//! The layout is private to this module and versioned ([`VERSION`]): bytes of another version
//! decode as none, and the session waits for the server as it would without a preview.

use std::sync::Arc;

use serde_json::{Map, Value};

use super::{Assistant, Attachment, Compaction, CompactionStatus, Finished, Message, ModelRef, Notice, NoticeKind, Outcome, Part, Retry, StepError, Tokens, ToolStatus};
use crate::mention::{MentionKind, UserMention};
use crate::prompt::PathAttachment;
use crate::quote::PromptComment;

/// Bumped with every change to the layout.
pub const VERSION: u8 = 2;
/// An inline file larger than this keeps its name and kind but not its bytes (the server's
/// copy fills it in once it answers): a preview stays a few pages of the database.
const INLINE_MAX: usize = 32 * 1024;

fn varint_len(mut v: usize) -> usize {
    let mut n = 1;
    while v >= 0x80 {
        v >>= 7;
        n += 1;
    }
    n
}

/// Encodes `messages`, oldest first.
pub fn encode(messages: &[Message]) -> Vec<u8> {
    let mut w = W(Vec::with_capacity(4096));
    w.0.push(VERSION);
    w.n(messages.len());
    for m in messages {
        message(&mut w, m);
    }
    w.0
}

/// Encodes the longest suffix of `messages` whose serialized length fits in `max_bytes`, always
/// keeping at least the last message when `messages` is non-empty. Walks backwards once instead of
/// repeatedly re-encoding shrinking suffixes (O(max_bytes) instead of O(N²)).
pub fn encode_suffix_within(messages: &[Message], max_bytes: usize) -> Vec<u8> {
    if messages.is_empty() {
        return encode(&[]);
    }
    let mut chunks: Vec<Vec<u8>> = Vec::new();
    let mut body_len = 0usize;
    for m in messages.iter().rev() {
        let next_count = chunks.len() + 1;
        if !chunks.is_empty()
            && 1usize
                .saturating_add(varint_len(next_count))
                .saturating_add(body_len)
                >= max_bytes
        {
            break;
        }
        let mut one = W(Vec::new());
        message(&mut one, m);
        let next_total = 1usize
            .saturating_add(varint_len(next_count))
            .saturating_add(body_len)
            .saturating_add(one.0.len());
        if !chunks.is_empty() && next_total > max_bytes {
            break;
        }
        body_len += one.0.len();
        chunks.push(one.0);
    }
    let mut w = W(Vec::with_capacity(1 + varint_len(chunks.len()) + body_len));
    w.0.push(VERSION);
    w.n(chunks.len());
    for chunk in chunks.iter().rev() {
        w.0.extend_from_slice(chunk);
    }
    w.0
}

/// Encodes at least `min_count` messages from the start of `messages`, then appends further
/// messages while the total length stays within `grow_limit`. Returns `None` if the encoded
/// result exceeds `hard_limit` (aborting early without encoding the full slice).
pub fn encode_prefix_within(
    messages: &[Message],
    min_count: usize,
    grow_limit: usize,
    hard_limit: usize,
) -> Option<Vec<u8>> {
    let min_count = min_count.min(messages.len());
    let mut body = W(Vec::with_capacity(4096.min(hard_limit)));
    for m in &messages[..min_count] {
        message(&mut body, m);
        if 1 + varint_len(min_count) + body.0.len() > hard_limit {
            return None;
        }
    }
    let mut count = min_count;
    for m in &messages[min_count..] {
        if 1 + varint_len(count + 1) + body.0.len() >= grow_limit {
            break;
        }
        let prev = body.0.len();
        message(&mut body, m);
        if 1 + varint_len(count + 1) + body.0.len() > grow_limit {
            body.0.truncate(prev);
            break;
        }
        count += 1;
    }
    if 1 + varint_len(count) + body.0.len() > hard_limit {
        return None;
    }
    let mut w = W(Vec::with_capacity(1 + varint_len(count) + body.0.len()));
    w.0.push(VERSION);
    w.n(count);
    w.0.extend_from_slice(&body.0);
    Some(w.0)
}

/// Decodes what [`encode`] wrote, putting each user message's files, path cards, comments,
/// mentions and send time back where the timeline reads them. None for other bytes.
pub fn decode(bytes: &[u8]) -> Option<Vec<Message>> {
    read(bytes, Extras::Restore)
}

/// Reads message bodies for an offline index. Unlike restoring a timeline, this never
/// populates global attachment, path or comment stores for sessions nobody opened.
pub fn decode_for_search(bytes: &[u8]) -> Option<Vec<Message>> {
    read(bytes, Extras::Skip)
}

/// How many messages [`encode`]'s bytes hold, read from their head. None for other bytes.
pub fn count(bytes: &[u8]) -> Option<usize> {
    let mut r = R { b: bytes, at: 0 };
    if r.u8()? != VERSION {
        return None;
    }
    r.n()
}

#[derive(Clone, Copy)]
enum Extras { Restore, Skip }

fn read(bytes: &[u8], extras: Extras) -> Option<Vec<Message>> {
    let mut r = R { b: bytes, at: 0 };
    if r.u8()? != VERSION {
        return None;
    }
    let n = r.n()?;
    let mut out = Vec::with_capacity(n.min(4096));
    for _ in 0..n {
        out.push(read_message(&mut r, extras)?);
    }
    (r.at == bytes.len()).then_some(out)
}

struct W(Vec<u8>);

impl W {
    fn n(&mut self, mut v: usize) {
        loop {
            let byte = (v & 0x7f) as u8;
            v >>= 7;
            if v == 0 {
                self.0.push(byte);
                return;
            }
            self.0.push(byte | 0x80);
        }
    }
    fn i(&mut self, v: i64) {
        self.n(((v << 1) ^ (v >> 63)) as usize);
    }
    fn b(&mut self, v: bool) {
        self.0.push(u8::from(v));
    }
    fn f(&mut self, v: f64) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    fn s(&mut self, v: &str) {
        self.bytes(v.as_bytes());
    }
    fn bytes(&mut self, v: &[u8]) {
        self.n(v.len());
        self.0.extend_from_slice(v);
    }
    fn os(&mut self, v: Option<&str>) {
        self.b(v.is_some());
        if let Some(v) = v {
            self.s(v);
        }
    }
    fn oi(&mut self, v: Option<i64>) {
        self.b(v.is_some());
        if let Some(v) = v {
            self.i(v);
        }
    }
}

struct R<'a> {
    b: &'a [u8],
    at: usize,
}

impl<'a> R<'a> {
    fn u8(&mut self) -> Option<u8> {
        let v = *self.b.get(self.at)?;
        self.at += 1;
        Some(v)
    }
    fn n(&mut self) -> Option<usize> {
        let mut v = 0usize;
        for shift in (0..64).step_by(7) {
            let byte = self.u8()?;
            v |= usize::from(byte & 0x7f) << shift;
            if byte & 0x80 == 0 {
                return Some(v);
            }
        }
        None
    }
    fn i(&mut self) -> Option<i64> {
        let v = self.n()? as u64;
        Some(((v >> 1) as i64) ^ -((v & 1) as i64))
    }
    fn b(&mut self) -> Option<bool> {
        Some(self.u8()? != 0)
    }
    fn f(&mut self) -> Option<f64> {
        let v = self.b.get(self.at..self.at + 8)?;
        self.at += 8;
        Some(f64::from_le_bytes(v.try_into().ok()?))
    }
    fn raw(&mut self) -> Option<&'a [u8]> {
        let n = self.n()?;
        let v = self.b.get(self.at..self.at.checked_add(n)?)?;
        self.at += n;
        Some(v)
    }
    fn s(&mut self) -> Option<String> {
        std::str::from_utf8(self.raw()?).ok().map(str::to_owned)
    }
    fn os(&mut self) -> Option<Option<String>> {
        if self.b()? { self.s().map(Some) } else { Some(None) }
    }
    fn oi(&mut self) -> Option<Option<i64>> {
        if self.b()? { self.i().map(Some) } else { Some(None) }
    }
}

fn message(w: &mut W, m: &Message) {
    match m {
        Message::User { id, text } => {
            w.0.push(0);
            w.s(id);
            w.s(text);
            user_extras(w, id);
        }
        Message::Assistant(a) => {
            w.0.push(1);
            assistant(w, a);
        }
        Message::Note { id, text, failed, step } => {
            w.0.push(2);
            w.s(id);
            w.s(text);
            w.b(*failed);
            assistant(w, step);
        }
        Message::Compaction(c) => {
            w.0.push(3);
            w.s(&c.id);
            w.0.push(match c.status {
                CompactionStatus::Running => 0,
                CompactionStatus::Completed => 1,
                CompactionStatus::Failed => 2,
            });
            w.b(c.manual);
            w.s(&c.summary);
            tokens(w, c.tokens.as_ref());
            w.b(c.native);
            w.b(c.error.is_some());
            if let Some((kind, message)) = &c.error {
                w.s(kind);
                w.s(message);
            }
            w.i(c.created);
        }
        Message::Notice(n) => {
            w.0.push(4);
            notice(w, n);
        }
    }
}

fn read_message(r: &mut R<'_>, extras: Extras) -> Option<Message> {
    Some(match r.u8()? {
        0 => {
            let (id, text) = (r.s()?, r.s()?);
            match extras { Extras::Restore => read_user_extras(r, &id)?, Extras::Skip => skip_extras(r)? }
            Message::User { id, text }
        }
        1 => Message::Assistant(read_assistant(r)?),
        2 => Message::Note { id: r.s()?, text: r.s()?, failed: r.b()?, step: Box::new(read_assistant(r)?) },
        3 => Message::Compaction(Compaction {
            id: r.s()?,
            status: match r.u8()? {
                0 => CompactionStatus::Running,
                1 => CompactionStatus::Completed,
                _ => CompactionStatus::Failed,
            },
            manual: r.b()?,
            summary: r.s()?,
            tokens: read_tokens(r)?,
            native: r.b()?,
            error: if r.b()? { Some((r.s()?, r.s()?)) } else { None },
            created: r.i()?,
        }),
        4 => Message::Notice(read_notice(r)?),
        _ => return None,
    })
}

/// What a user message keeps beside its text, from the stores the timeline reads.
fn user_extras(w: &mut W, id: &str) {
    files(w, &super::user_files(id));
    let paths = crate::mention::user_path_attachments(id);
    w.n(paths.len());
    for p in paths.iter() {
        w.s(&p.name);
        w.s(&p.mime);
        w.s(&p.path);
    }
    let comments = crate::quote::user_comments(id);
    w.s(&if comments.is_empty() { String::new() } else { serde_json::to_string(&*comments).unwrap_or_default() });
    let mentions = crate::mention::user_mentions(id);
    w.n(mentions.len());
    for m in mentions.iter() {
        w.n(m.range.start);
        w.n(m.range.end);
        w.0.push(match m.kind {
            MentionKind::Agent => 0,
            MentionKind::Skill => 1,
            MentionKind::File => 2,
            MentionKind::Directory => 3,
            MentionKind::Session => 4,
        });
    }
    w.oi(super::user_created(id));
}

fn skip_extras(r: &mut R<'_>) -> Option<()> {
    for _ in 0..r.n()? { r.raw()?; r.raw()?; r.raw()?; if r.b()? { r.raw()?; } }
    for _ in 0..r.n()? { r.raw()?; r.raw()?; r.raw()?; }
    r.raw()?;
    for _ in 0..r.n()? { r.n()?; r.n()?; r.u8()?; }
    r.oi()?;
    Some(())
}

fn read_user_extras(r: &mut R<'_>, id: &str) -> Option<()> {
    let files = read_files(r)?;
    if !files.is_empty() {
        super::set_user_files(id, files);
    }
    let paths = (0..r.n()?).map(|_| Some(PathAttachment { name: r.s()?, mime: r.s()?, path: r.s()? })).collect::<Option<Vec<_>>>()?;
    if !paths.is_empty() {
        crate::mention::set_user_path_attachments(id, paths);
    }
    // Comments are few and rare: their own small JSON.
    let comments = r.s()?;
    if !comments.is_empty() {
        crate::quote::set_user_comments(id, serde_json::from_str::<Vec<PromptComment>>(&comments).unwrap_or_default());
    }
    let mentions = (0..r.n()?)
        .map(|_| {
            let range = r.n()?..r.n()?;
            let kind = match r.u8()? {
                0 => MentionKind::Agent,
                1 => MentionKind::Skill,
                2 => MentionKind::File,
                3 => MentionKind::Directory,
                _ => MentionKind::Session,
            };
            Some(UserMention { range, kind })
        })
        .collect::<Option<Vec<_>>>()?;
    if !mentions.is_empty() {
        crate::mention::set_user_mentions(id, mentions);
    }
    if let Some(at) = r.oi()? {
        super::set_user_created(id, at);
    }
    Some(())
}

fn files(w: &mut W, files: &[Attachment]) {
    w.n(files.len());
    for f in files {
        w.s(&f.mime);
        w.s(&f.name);
        w.bytes(if f.bytes.len() <= INLINE_MAX { &f.bytes } else { &[] });
        w.os(f.media.as_deref());
    }
}

fn read_files(r: &mut R<'_>) -> Option<Vec<Attachment>> {
    (0..r.n()?).map(|_| Some(Attachment { mime: r.s()?, name: r.s()?, bytes: Arc::from(r.raw()?), media: r.os()? })).collect()
}

fn tokens(w: &mut W, t: Option<&Tokens>) {
    w.b(t.is_some());
    if let Some(t) = t {
        for v in [t.input, t.output, t.cache_read, t.cache_write, t.reasoning] {
            w.n(v as usize);
        }
    }
}

fn read_tokens(r: &mut R<'_>) -> Option<Option<Tokens>> {
    if !r.b()? {
        return Some(None);
    }
    let mut n = || r.n().map(|v| v as u64);
    Some(Some(Tokens { input: n()?, output: n()?, cache_read: n()?, cache_write: n()?, reasoning: n()? }))
}

fn model(w: &mut W, m: Option<&ModelRef>) {
    w.b(m.is_some());
    if let Some(m) = m {
        w.s(&m.id);
        w.s(&m.provider);
        w.s(&m.variant);
    }
}

fn read_model(r: &mut R<'_>) -> Option<Option<ModelRef>> {
    if !r.b()? {
        return Some(None);
    }
    Some(Some(ModelRef { id: r.s()?, provider: r.s()?, variant: r.s()? }))
}

fn assistant(w: &mut W, a: &Assistant) {
    w.s(&a.id);
    w.oi(a.completed);
    w.n(a.parts.len());
    for p in &a.parts {
        part(w, p);
    }
    w.f(a.cost);
    tokens(w, a.tokens.as_ref());
    w.os(a.error.as_deref());
    w.b(a.failure.is_some());
    if let Some(e) = &a.failure {
        w.s(&e.kind);
        w.s(&e.message);
        w.oi(e.status.map(i64::from));
        w.oi(e.retry_after_ms);
        w.os(e.body.as_deref());
    }
    w.b(a.retry.is_some());
    if let Some(t) = &a.retry {
        w.n(t.attempt as usize);
        w.i(t.at);
        w.s(&t.message);
    }
    w.i(a.created);
    w.oi(a.first_token);
    w.oi(a.streamed);
    model(w, a.model.as_ref());
    w.os(a.agent.as_deref());
    w.n(a.changed.len());
    for c in &a.changed {
        w.s(c);
    }
}

fn read_assistant(r: &mut R<'_>) -> Option<Assistant> {
    let id = r.s()?;
    let completed = r.oi()?;
    let parts = (0..r.n()?).map(|_| read_part(r)).collect::<Option<Vec<_>>>()?;
    let cost = r.f()?;
    let tokens = read_tokens(r)?;
    let error = r.os()?;
    let failure = if r.b()? {
        Some(StepError { kind: r.s()?, message: r.s()?, status: r.oi()?.and_then(|s| u16::try_from(s).ok()), retry_after_ms: r.oi()?, body: r.os()? })
    } else {
        None
    };
    let retry = if r.b()? { Some(Retry { attempt: u32::try_from(r.n()?).ok()?, at: r.i()?, message: r.s()? }) } else { None };
    Some(Assistant { id, completed, parts, text_phases: Vec::new(), cost, tokens, error, failure, retry, created: r.i()?, first_token: r.oi()?, streamed: r.oi()?, model: read_model(r)?, agent: r.os()?, changed: (0..r.n()?).map(|_| r.s()).collect::<Option<Vec<_>>>()? })
}

fn part(w: &mut W, p: &Part) {
    match p {
        Part::Text { text } => {
            w.0.push(0);
            w.s(text);
        }
        Part::Reasoning { text, started, completed } => {
            w.0.push(1);
            w.s(text);
            w.i(*started);
            w.oi(*completed);
        }
        Part::Tool { id, name, status, title, input, files: f, metadata } => {
            w.0.push(2);
            w.s(id);
            w.s(name);
            w.0.push(match status {
                ToolStatus::Pending => 0,
                ToolStatus::Running => 1,
                ToolStatus::Completed => 2,
                ToolStatus::Error => 3,
            });
            w.s(title);
            value(w, input);
            files(w, f);
            value(w, metadata);
        }
    }
}

fn read_part(r: &mut R<'_>) -> Option<Part> {
    Some(match r.u8()? {
        0 => Part::Text { text: r.s()? },
        1 => Part::Reasoning { text: r.s()?, started: r.i()?, completed: r.oi()? },
        2 => Part::Tool {
            id: r.s()?,
            name: r.s()?,
            status: match r.u8()? {
                0 => ToolStatus::Pending,
                1 => ToolStatus::Running,
                2 => ToolStatus::Completed,
                _ => ToolStatus::Error,
            },
            title: r.s()?,
            input: read_value(r, 0)?,
            files: read_files(r)?,
            metadata: read_value(r, 0)?,
        },
        _ => return None,
    })
}

fn notice(w: &mut W, n: &Notice) {
    w.s(&n.id);
    match &n.kind {
        NoticeKind::Agent { agent, previous } => {
            w.0.push(0);
            w.s(agent);
            w.os(previous.as_deref());
        }
        NoticeKind::Model { model: m, previous } => {
            w.0.push(1);
            model(w, Some(m));
            model(w, previous.as_ref());
        }
        NoticeKind::Moved { directory } => {
            w.0.push(2);
            w.s(directory);
        }
        NoticeKind::Instructions { sources } => {
            w.0.push(3);
            w.n(sources.len());
            for s in sources {
                w.s(s);
            }
        }
        NoticeKind::Restart => w.0.push(4),
        NoticeKind::Finished(f) => {
            w.0.push(5);
            w.os(f.agent.as_deref());
            w.b(f.subagent);
            w.0.push(match f.outcome {
                Outcome::Completed => 0,
                Outcome::Error => 1,
                Outcome::Cancelled => 2,
            });
            w.os(f.data.as_deref());
            w.b(f.required);
            w.os(f.shell.as_deref());
            w.os(f.child.as_deref());
        }
        NoticeKind::Text(t) => {
            w.0.push(6);
            w.s(t);
        }
        NoticeKind::Idle => w.0.push(7),
        NoticeKind::Job { id, description, script: None, required: false } => {
            w.0.push(8);
            w.s(id);
            w.s(description);
        }
        NoticeKind::Job { id, description, script: Some(outcome), .. } => {
            w.0.push(9);
            w.s(id);
            w.s(description);
            w.0.push(match outcome {
                Outcome::Completed => 0,
                Outcome::Error => 1,
                Outcome::Cancelled => 2,
            });
        }
        NoticeKind::Job { id, description, script: None, required: true } => {
            w.0.push(10);
            w.s(id);
            w.s(description);
        }
    }
}

fn read_notice(r: &mut R<'_>) -> Option<Notice> {
    let id = r.s()?;
    let kind = match r.u8()? {
        0 => NoticeKind::Agent { agent: r.s()?, previous: r.os()? },
        1 => NoticeKind::Model { model: read_model(r)??, previous: read_model(r)? },
        2 => NoticeKind::Moved { directory: r.s()? },
        3 => NoticeKind::Instructions { sources: (0..r.n()?).map(|_| r.s()).collect::<Option<Vec<_>>>()? },
        4 => NoticeKind::Restart,
        5 => NoticeKind::Finished(Finished {
            agent: r.os()?,
            subagent: r.b()?,
            outcome: match r.u8()? {
                0 => Outcome::Completed,
                1 => Outcome::Error,
                _ => Outcome::Cancelled,
            },
            data: r.os()?,
            required: r.b()?,
            shell: r.os()?,
            child: r.os()?,
        }),
        6 => NoticeKind::Text(r.s()?),
        7 => NoticeKind::Idle,
        8 => NoticeKind::Job { id: r.s()?, description: r.s()?, script: None, required: false },
        9 => {
            let id = r.s()?;
            let description = r.s()?;
            let outcome = match r.u8()? {
                0 => Outcome::Completed,
                1 => Outcome::Error,
                _ => Outcome::Cancelled,
            };
            NoticeKind::Job { id, description, script: Some(outcome), required: outcome == Outcome::Error }
        }
        10 => NoticeKind::Job { id: r.s()?, description: r.s()?, script: None, required: true },
        _ => return None,
    };
    Some(Notice { id, kind })
}

/// A tool's input or state as a tagged tree (no text to parse back).
fn value(w: &mut W, v: &Value) {
    match v {
        Value::Null => w.0.push(0),
        Value::Bool(b) => w.0.push(if *b { 2 } else { 1 }),
        Value::Number(n) => match (n.as_i64(), n.as_f64()) {
            (Some(i), _) => {
                w.0.push(3);
                w.i(i);
            }
            (None, Some(f)) => {
                w.0.push(4);
                w.f(f);
            }
            _ => w.0.push(0),
        },
        Value::String(s) => {
            w.0.push(5);
            w.s(s);
        }
        Value::Array(a) => {
            w.0.push(6);
            w.n(a.len());
            for x in a {
                value(w, x);
            }
        }
        Value::Object(o) => {
            w.0.push(7);
            w.n(o.len());
            for (k, x) in o {
                w.s(k);
                value(w, x);
            }
        }
    }
}

fn read_value(r: &mut R<'_>, depth: usize) -> Option<Value> {
    if depth > 128 {
        return None;
    }
    Some(match r.u8()? {
        0 => Value::Null,
        1 => Value::Bool(false),
        2 => Value::Bool(true),
        3 => Value::from(r.i()?),
        4 => serde_json::Number::from_f64(r.f()?).map_or(Value::Null, Value::Number),
        5 => Value::String(r.s()?),
        6 => Value::Array((0..r.n()?).map(|_| read_value(r, depth + 1)).collect::<Option<Vec<_>>>()?),
        7 => {
            let n = r.n()?;
            let mut m = Map::new();
            for _ in 0..n {
                let k = r.s()?;
                m.insert(k, read_value(r, depth + 1)?);
            }
            Value::Object(m)
        }
        _ => return None,
    })
}
