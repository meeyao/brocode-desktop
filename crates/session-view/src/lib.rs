//! Shared OpenCode session timeline (`p255`).
//!
//! Renders an [`opencode_kit::model::Transcript`] in a virtualized, bottom-following timeline with
//! user bubbles (long-prompt folding, `@` mention badges, quote cards, images, hover copy),
//! assistant turns (Markdown, `genui`, Mermaid diagrams, reasoning, folded tool groups, subagent
//! cards, question rows, inline images, turn errors/retries, compactions, notices, streamed line
//! reveal, Working/Copy/Interrupted tail), linear bottom fade, two-layer live overlay scroll bar,
//! and "Jump to latest", with no Brocode-only shell or side-panel assumptions inside.

pub use opencode_runtime::detail;
pub use opencode_ui::keymap::base as keymap;
pub use opencode_ui::{dialog, focus, odometer, palette, scroller, sheet, themes};

pub mod activity;
pub mod agent_start;
pub mod arrivals;
pub mod compaction;
pub mod composer;
pub mod copied;
pub mod diagram;
pub mod disclosure;
pub mod dock;
pub mod fixture;
pub mod fold;
pub mod follow;
pub mod genui;
pub mod images;
mod video;
pub mod jump;
pub mod knobs;
pub mod line_reveal;
pub mod pickers;
pub mod popover;
pub mod question_row;
pub mod recall;
pub mod returning;
pub mod session_chips;
pub mod sources_chip;
pub mod story;
pub mod style;
pub mod subagent_card;
pub mod timeline;
pub mod title;
pub mod tools;
pub mod traits;
pub mod turn_diff;
pub mod turn_errors;
pub mod turns;
pub mod usage;
pub mod user_parts;
pub mod user_quotes;
pub mod working;

use std::ops::Range;

use opencode_kit::client::Request;
use opencode_kit::model::{Inbox, InboxItem, ModelRef, ToolStatus, Transcript};
use rune::{Cx, El, ScrollPos, SelectionChange, TextHighlight, Tx, Waker};
use rune_knobs::Knobs;

pub use traits::{TimelineActions, TimelineClock, TimelineStateAccess, TranscriptSource};

/// Bundled per-window state for a host that embeds a session timeline.
#[derive(Default)]
pub struct ViewState {
    pub expanded: disclosure::Disclosures,
    pub copied: copied::Copied,
    pub working: working::Working,
    pub diagram: diagram::Diagrams,
    pub media: images::MediaStore,
    pub line_hold: line_reveal::Hold,
    pub ends: jump::Ends,
    pub composer: composer::ComposerState,
    /// This window's part in display math: its wake and what it has seen finish.
    pub math: rune_kit::math::Client,
}

impl ViewState {
    /// Connects the background waker for media decoding and display math.
    pub fn setup(&mut self, waker: Waker, _headless: bool) {
        self.math.set_waker(waker.clone());
        self.media.set_waker(waker);
    }
}

/// Implements [`TimelineHost`]'s state-accessor pairs by delegating to a [`ViewState`] field.
#[macro_export]
macro_rules! delegate_view_state {
    ($field:ident) => {
        fn expanded(&self) -> &$crate::disclosure::Disclosures { &self.$field.expanded }
        fn expanded_mut(&mut self) -> &mut $crate::disclosure::Disclosures { &mut self.$field.expanded }
        fn copied_state(&self) -> &$crate::copied::Copied { &self.$field.copied }
        fn copied_state_mut(&mut self) -> &mut $crate::copied::Copied { &mut self.$field.copied }
        fn working_state(&self) -> &$crate::working::Working { &self.$field.working }
        fn working_state_mut(&mut self) -> &mut $crate::working::Working { &mut self.$field.working }
        fn diagrams(&self) -> &$crate::diagram::Diagrams { &self.$field.diagram }
        fn diagrams_mut(&mut self) -> &mut $crate::diagram::Diagrams { &mut self.$field.diagram }
        fn media_store(&self) -> &$crate::images::MediaStore { &self.$field.media }
        fn media_store_mut(&mut self) -> &mut $crate::images::MediaStore { &mut self.$field.media }
        fn line_hold(&self) -> &$crate::line_reveal::Hold { &self.$field.line_hold }
        fn line_hold_mut(&mut self) -> &mut $crate::line_reveal::Hold { &mut self.$field.line_hold }
        fn ends(&self) -> &$crate::jump::Ends { &self.$field.ends }
        fn ends_mut(&mut self) -> &mut $crate::jump::Ends { &mut self.$field.ends }
        fn math_client(&self) -> Option<&rune_kit::math::Client> { Some(&self.$field.math) }
    };
}

/// Host state and callbacks required by interactive timeline parts and overlays.
pub trait TimelineHost: Sized + 'static {
    fn knobs(&self) -> &Knobs;
    fn clock(&self) -> f64;
    fn now_ms(&self) -> i64 {
        0
    }
    fn reduced_motion(&self) -> bool {
        false
    }
    /// Animate a live delivery's Pending label in addition to its row geometry.
    fn smooth_timeline(&self) -> bool {
        false
    }

    fn active_session_id(&self) -> Option<&str>;
    fn transcript(&self, _session: &str) -> Option<&Transcript> {
        None
    }
    fn with_transcript<R>(&self, session: &str, f: impl FnOnce(&Transcript) -> R) -> Option<R> {
        Self::transcript(self, session).map(f)
    }
    /// A number that moves whenever session `session`'s transcript may have changed (a
    /// `Tracked` cell's version), so the timeline keeps its projection while it holds. `None`,
    /// the default: the host can't say, and the timeline projects its rows on every build.
    fn transcript_version(&self, _session: &str) -> Option<u64> {
        None
    }
    /// Session `session`'s first message that may have changed since `transcript_version` read
    /// `since` (past the end when none did). `None`, the default: the host can't say, and every
    /// span's messages are compared.
    fn transcript_changed_from(&self, _session: &str, _since: u64) -> Option<usize> {
        None
    }
    fn session_directory(&self, _session: &str) -> Option<&str> {
        None
    }

    /// Save the original bytes, not the cleaned/shortened text in the console. Resolve the
    /// result on press, so a closed card retains neither a second copy nor an open file.
    fn save_tool_output(&mut self, session: &str, id: &str, tx: &mut rune::Tx) {
        let Some(raw) = tools::raw_output(self, session, id) else { return };
        #[cfg(not(target_arch = "wasm32"))]
        {
            let options = rune::FileDialogOptions { title: opencode_i18n::t!("rune.scale.save").into(), file_name: Some("output.txt".into()), ..Default::default() };
            tx.save_file(options, move |s: &mut Self, answer, _| match answer {
                Ok(Some(path)) => {
                    let report = s.output_save_reporter();
                    if let Err(e) = rune::task::spawn("save-tool-output", move || {
                        use std::io::Write;
                        let write = || -> std::io::Result<()> {
                            let dir = path.parent().ok_or(std::io::ErrorKind::InvalidInput)?;
                            let mut temp = tempfile::NamedTempFile::new_in(dir)?;
                            temp.write_all(raw.as_bytes())?;
                            temp.as_file().sync_all()?;
                            temp.persist(path)?;
                            Ok(())
                        };
                        if let Err(e) = write() { report(e.to_string()); }
                    }) { s.output_save_failed(e.to_string()); }
                }
                Err(e) => s.output_save_failed(e.to_string()),
                Ok(None) => {}
            });
        }
        #[cfg(target_arch = "wasm32")]
        {
            let _ = tx;
            if let Err(e) = opencode_kit::client::save_download("output.txt", &raw) { self.output_save_failed(e); }
        }
    }

    fn output_save_failed(&mut self, reason: String) { eprintln!("save output: {reason}"); }
    fn output_save_reporter(&mut self) -> Box<dyn FnOnce(String) + Send> { Box::new(|why| eprintln!("save output: {why}")) }
    fn session_title(&self, _session: &str) -> Option<String> {
        None
    }
    /// Hosts with a local outbox can exclude undelivered bubbles from run status.
    fn timeline_busy(&self, transcript: &opencode_kit::model::Transcript) -> bool { transcript.busy() }

    fn is_session_live(&self, session: &str) -> bool {
        Self::with_transcript(self, session, Transcript::busy).unwrap_or(false)
    }
    fn is_session_stopped(&self, _session: &str) -> bool {
        false
    }
    /// The words in place of "Working" under a session that is still being started (Desktop's
    /// `PreparingSession` status line: "Creating worktree", "Starting session").
    fn working_label_for(&self, _session: &str) -> Option<&'static str> {
        None
    }
    /// Whether session `session` itself waits on a question or a permission, which hides its
    /// Working (`message-timeline.tsx:339`).
    fn session_asking(&self, session: &str) -> bool {
        Self::session_waiting(self, session).is_some()
    }
    /// What session `session` waits on the user for, which its status line says
    /// (`timeline-next/status.ts`): a permission, or an answer to a question.
    fn session_waiting(&self, _session: &str) -> Option<working::Waiting> {
        None
    }
    fn session_revert(&self, _session: &str) -> Option<&str> {
        None
    }
    fn for_each_tracked_session(&self, mut f: impl FnMut(&str)) {
        if let Some(id) = Self::active_session_id(self) {
            f(id);
        }
    }

    /// Whether a plugin renders fenced code blocks of `language` (Desktop's `CodeBlock`): a
    /// reply's ```` ```ui ```` outlines draw as parts only while one does.
    fn code_block_lang(&self, language: &str) -> bool {
        language == "ui"
    }
    /// The plugin's view of one such block, `code` its source between the fences, `key` its
    /// place in the reply; `native` is the host's own view of it (an outline), which the plugin
    /// places (`ui.genUi`), and `version` changes whenever `native` would. None keeps the
    /// block as code.
    #[allow(clippy::too_many_arguments)]
    fn code_block(&self, _p: &palette::Pal, _language: &str, _key: &str, _code: &str, _streaming: bool, _version: u64, native: El) -> Option<El> {
        Some(native)
    }

    fn expanded(&self) -> &disclosure::Disclosures;
    fn expanded_mut(&mut self) -> &mut disclosure::Disclosures;

    fn copied_state(&self) -> &copied::Copied;
    fn copied_state_mut(&mut self) -> &mut copied::Copied;

    fn working_state(&self) -> &working::Working;
    fn working_state_mut(&mut self) -> &mut working::Working;

    fn diagrams(&self) -> &diagram::Diagrams;
    fn diagrams_mut(&mut self) -> &mut diagram::Diagrams;

    fn media_store(&self) -> &images::MediaStore;
    fn media_store_mut(&mut self) -> &mut images::MediaStore;

    fn line_hold(&self) -> &line_reveal::Hold;
    fn line_hold_mut(&mut self) -> &mut line_reveal::Hold;

    fn ends(&self) -> &jump::Ends;
    fn ends_mut(&mut self) -> &mut jump::Ends;

    /// This window's display math client (`rune_kit::math::Client`): its wake when an equation
    /// finishes rendering, and its say in when cached bitmaps go. `None`: the host doesn't follow
    /// renders, and equations show once something else builds their row.
    fn math_client(&self) -> Option<&rune_kit::math::Client> {
        None
    }

    fn fold_user_messages(&self) -> bool {
        true
    }
    /// A sent prompt shows its quotes and comments inline in its bubble (`user_quotes`) instead of
    /// as comment cards. Desktop turns it on in local and dev builds only
    /// (`packages/app/src/session/timeline/inline-quotes.ts`).
    fn inline_quotes(&self) -> bool {
        false
    }
    /// Desktop supplies contextual actions and resolves media on the session's own machine.
    fn reply_link_menu(&mut self, _url: &str, _press: rune::PointerPress, _tx: &mut Tx) -> bool { false }
    fn reply_video_source(&self, _url: &str) -> Option<String> { None }
    fn load_reply_video(&mut self, _url: &str, _tx: &mut Tx) -> Option<String> { None }

    fn open_link(&mut self, _url: &str, _tx: &mut Tx) {}
    /// A right-click in the timeline on `at`: a message, an image, or an image in a message.
    /// By default an image gets its own items (`images::menu`) and a message nothing.
    fn context_menu(&mut self, at: images::menu::MenuAt, press: rune::PointerPress, tx: &mut Tx) {
        images::menu::open_default(self, at, press, tx);
    }
    /// Opens the file at `path` in its default app (an image's Open in Preview).
    fn open_file(&mut self, path: std::path::PathBuf) {
        images::menu::open_with_system(&path);
    }
    /// Shows the file at `path` selected in the file manager (Desktop's `revealPath`); false
    /// when it couldn't.
    fn reveal_file(&mut self, path: &std::path::Path) -> bool {
        images::menu::reveal_with_system(path)
    }
    /// The output the host read from shell shell while its console shows (`tools::shell_follow`),
    /// which the console shows in place of its call's.
    fn shell_output(&self, _shell: &str) -> Option<&str> {
        None
    }
    /// Changes whenever any [`TimelineHost::shell_output`] does, so the rows showing them build again.
    fn shell_output_version(&self) -> u64 {
        0
    }
    fn reply_link_help(_url: &str) -> Option<String> {
        None
    }
    /// The inline code in finished reply `body` that names a file the host found, which draws as
    /// a link (Desktop's `markInlineCode`); `None` while there is none or the reply streams.
    fn code_links(&self, _body: &str, _live: bool) -> Option<std::rc::Rc<std::collections::HashSet<String>>> {
        None
    }
    /// Changes whenever a [`TimelineHost::code_links`] answer may have, so rows with code build again.
    fn code_links_version(&self) -> u64 {
        0
    }
    /// A reply's Markdown as the host wants it drawn, e.g. its own ids made links; `None` draws
    /// it as written. Find highlights index the text as written, so a host that rewrites
    /// shouldn't also use find.
    fn rewrite_markdown(_src: &str) -> Option<String> {
        None
    }
    /// Extra hover buttons for a finished code block in `language` holding `source`, drawn left
    /// of its Copy button (a host's plugins' code block actions). Markdown draws code blocks
    /// without the host's state, so these come from what the host registered up front.
    fn code_block_actions(_p: &palette::Pal, _language: Option<&str>, _source: &str) -> Vec<El> {
        Vec::new()
    }
    fn send_request(&self, _req: Request) {}
    fn can_switch_model(&self) -> bool {
        false
    }
    fn switch_model(&mut self, _tx: &mut Tx) {}
    /// Returns focus to the composer after a keyboard-triggered Jump to latest (`virtualizer.tsx:1171`).
    fn focus_composer(&mut self, _tx: &mut Tx) {}
    /// A move from `previous` to `model` that changed the model's speed, not the model: the
    /// speed it moved to, none for its standard one (`pickers::Pickers::speed_switch`).
    fn model_speed_switch(&self, _model: &ModelRef, _previous: &ModelRef) -> Option<Option<String>> {
        None
    }
    fn model_name(&self, model: &ModelRef) -> String {
        if model.id.is_empty() {
            model.provider.clone()
        } else {
            model.id.clone()
        }
    }
    /// A button beside call `call`'s row in session `session`: it opens what the call touched in
    /// the host's side panel (a read file at its lines); `None` for none. A press on the row
    /// itself opens the call inline. Rest its glyph hidden with `group_opacity(0.0, 1.0)`, so it
    /// shows while the tool is hovered and stays pressable. An edit card's file rows take none,
    /// as Desktop's `accordion-trigger` has none.
    fn tool_row_action(&self, _p: &palette::Pal, _session: &str, _call: &tools::Call) -> Option<El> {
        None
    }
    /// Included in a tool row's memo key: what [`Self::tool_row_action`] shows for call `id` can
    /// change without the call changing.
    fn tool_row_action_version(&self, _id: &str) -> u64 {
        0
    }

    fn can_open_subagent(&self) -> bool {
        false
    }
    /// The subagent call whose row the pointer is over (`subagent_card`), whose chevron slides in
    /// as Desktop's `task-tool-action` does; `None` without one, or in a host that doesn't track it.
    fn subagent_hover(&self) -> Option<&str> {
        None
    }
    fn set_subagent_hover(&mut self, _call: Option<String>) {}
    fn resolve_subagent<'a>(&'a self, _parent: &str, session: Option<&'a str>, _title: &str) -> Option<&'a str> {
        session
    }
    fn subagent_run(&self, _parent: &str, _call: &str, _child: Option<&str>, status: &ToolStatus) -> subagent_card::Run {
        subagent_card::Run::from_status(status)
    }
    fn prefetch_subagent(&self, _child: &str) {}
    fn open_subagent(&mut self, _parent: &str, _child: &str, _title: &str, _agent: &str, _tx: &mut Tx) {}
    /// Whether a session mention's badge in a prompt opens that session ([`open_session`]).
    ///
    /// [`open_session`]: TimelineHost::open_session
    fn can_open_session(&self) -> bool {
        false
    }
    fn open_session(&mut self, _id: &str, _tx: &mut Tx) {}
    /// What session `id`, named in a reply or a prompt, stands for: `None` while the host doesn't
    /// know it, and for good when the server doesn't either, so it stays text. A host that
    /// looks one up remembers asking, as this runs on every build of the row.
    fn session_ref(&self, _id: &str) -> Option<session_chips::SessionRef> {
        None
    }
    /// A right-click on the chip of session `id` (`session_chips`): its menu at `press`.
    fn session_chip_menu(&mut self, _id: &str, _press: rune::PointerPress, _tx: &mut Tx) {}

    fn loading_error(&self, _session: &str) -> Option<&str> {
        None
    }
    /// The loading row's Retry, after `loading_error`: read the messages again.
    fn retry_loading(&mut self, _session: &str, _tx: &mut Tx) {}
    fn partial_output(&self, _session:&str) -> Option<&str>{None}
    fn loading_older(&self, _session: &str) -> bool {
        false
    }
    fn more_above(&self, _session: &str) -> bool {
        false
    }
    /// Whether an assistive client reads the window: what only a screen reader uses (a turn's
    /// heading) is built only then, and costs nothing without one. A change re-runs the rows.
    fn assistive(&self) -> bool {
        true
    }
    #[allow(clippy::type_complexity)]
    fn watch_scroll_top(&self, _session: &str) -> Option<Box<dyn Fn(ScrollPos)>> {
        None
    }
    fn on_timeline_selection_change(_s: &mut Self, _change: &SelectionChange, _tx: &mut Tx) {}
    /// Key handler checked on the timeline viewport ahead of Rune's `caret_browsing` keys when
    /// the chat timeline holds focus (`tx.focused() == Some("timeline")`).
    fn on_timeline_key(&mut self, _ev: &rune::KeyEvent, _tx: &mut Tx) -> bool {
        false
    }

    fn find_row_version(&self, _row_key: &str) -> u64 {
        0
    }
    fn find_text_highlights(&self, _p: &palette::Pal, _row_key: &str) -> Vec<TextHighlight> {
        Vec::new()
    }
    fn find_markdown_highlights(&self, _p: &palette::Pal, _row_key: &str) -> Vec<(&[Range<usize>], rune_kit::Highlight)> {
        Vec::new()
    }
    #[allow(clippy::type_complexity)]
    fn find_row_hits(&self, _row_key: &str) -> Option<(&[Vec<Range<usize>>], Option<usize>)> {
        None
    }

    fn user_glow_at(&self, _id: &str) -> Option<f64> {
        None
    }
    fn user_menu_open_on(&self, _id: &str) -> bool {
        false
    }
    fn quote_more_open(&self, id: &str) -> bool {
        Self::expanded(self).contains(&format!("quote-more:{id}"))
    }
    fn open_quote_more(&mut self, id: String) {
        Self::expanded_mut(self).insert(format!("quote-more:{id}"));
    }
    /// The button in a prompt's top trailing corner (`timeline::user_menu_button`), which opens
    /// its actions; `pending` for a steer the server hasn't delivered. `None` for none.
    fn user_menu(&self, p: &palette::Pal, _session: &str, id: &str, body: &str, _pending: bool) -> Option<El> {
        timeline::default_user_menu(self, p, id, body)
    }
    /// Delivery status belongs below its own bubble, even inside a run of user messages.
    fn user_status(&self, _p: &palette::Pal, _id: &str) -> Option<El> { None }
    /// Included in the user-row memo key; a receipt can change without changing the message.
    fn user_status_version(&self, _id: &str) -> u64 { 0 }

    fn inbox_for(&self, _session: &str) -> Option<&Inbox> {
        None
    }
    fn queued_compaction(&self, _session: &str) -> Option<&str> {
        None
    }
    fn pending_steer_label(&self, item: &InboxItem) -> String {
        item.text.clone()
    }
    /// What sits beside the status line in a turn's lane, 8 after its words and centred on it
    /// (Brocode's "Move to background" hint).
    fn turn_tail_extra(&self, _p: &palette::Pal, _session: &str, _busy: bool) -> Option<El> {
        None
    }

    /// Settings › Preferences › Timeline detail: what of a turn's work shows, and how.
    fn timeline_detail(&self) -> detail::Detail {
        detail::Detail::default()
    }
    /// Settings › Preferences › Changed files: a finished turn lists the files it changed.
    fn turn_diffs(&self) -> bool {
        false
    }
    /// Settings › Preferences › Wrap code blocks (`general.wrapCodeBlocks`, anomalyco/opencode#64545).
    fn wrap_code_blocks(&self) -> bool {
        false
    }
    /// Per-block code-wrap overrides keyed by `"{row_key}:{block_index}"`.
    fn code_wrap_overrides(&self) -> Option<&std::collections::HashMap<String, bool>> {
        None
    }
    /// Sets or clears a per-block code-wrap override (`None` resumes following `wrap_code_blocks`).
    fn set_code_wrap_override(&mut self, _key: String, _override: Option<bool>) {}
    /// The diff of session `session`'s run from prompt `from`, for Changed files; a host loads
    /// it when first asked.
    fn turn_diff(&self, _session: &str, _from: &str) -> turn_diff::State<'_> {
        turn_diff::State::Failed
    }
    /// Loads that diff again after a failure.
    fn retry_turn_diff(&mut self, _session: &str, _from: &str) {}
    /// The key of the pane the timeline scrolls in, header and composer included: Desktop's
    /// timeline viewport runs under both, and Jump to latest shows past its height from the end.
    /// Rune measures it as the timeline scrolls, so no view reads it. None: the timeline's own.
    fn timeline_frame(&self) -> Option<&'static str> {
        None
    }
    /// Settings › Preferences › Background activity: where the pills over the composer sit.
    fn lane_align(&self) -> jump::Align {
        jump::Align::Start
    }
    /// Settings › Preferences › Timeline width: the column the timeline draws at.
    fn content_width(&self) -> f32 {
        style::CONTENT_W
    }
}

/// Full timeline region: virtualized message stack, linear bottom fade, live overlay scroll bar,
/// and floating `pills` + "Jump to latest" button. A session `header` sticks over the
/// timeline's top as rows scroll under it (`timeline::timeline`).
pub fn timeline_area<S: TimelineHost>(s: &S, p: &palette::Pal, id: &str, t: Option<&Transcript>, pills: Vec<El>, header: Option<El>) -> El {
    style::set_content_w(s.content_width());
    let lane = jump::lane(!pills.is_empty());
    let top = if header.is_some() { style::HEADER_H } else { 0.0 };
    jump::over(s, p, id, timeline::timeline(s, p, id, t, lane, header), pills, top)
}

/// Read-only session timeline region (`timeline_area` with no composer activity pills).
pub fn read_only<S: TimelineHost>(s: &S, p: &palette::Pal, id: &str, t: Option<&Transcript>) -> El {
    diagram::prepare(s);
    timeline_area(s, p, id, t, Vec::new(), None)
}

/// Modal overlays opened from timeline rows (full-size image viewer, Mermaid diagram lightbox and
/// Execute's View code dialog).
pub fn overlays<S: TimelineHost>(s: &S, p: &palette::Pal, cx: &mut Cx<S>) -> Vec<El> {
    let mut out = Vec::new();
    out.extend(images::viewer(s, p, cx));
    out.extend(diagram::view(s, p, cx));
    out.extend(tools::details_overlay(s, p, cx));
    out
}

/// A press with no modifier key held. A link to another session in the timeline opens only on
/// one: Desktop's `navigate` returns on `altKey || ctrlKey || metaKey || shiftKey`
/// (`tool-renderer.tsx`, `session-timeline-row.tsx`), and its window-open handler refuses the
/// app's own address a modified click would open.
pub(crate) fn plain_click(tx: &Tx) -> bool {
    let m = tx.mods();
    !(m.shift || m.alt || m.ctrl || m.cmd || m.logo)
}

/// Advances all shared timeline animations and timers for one frame.
pub fn poll<S: TimelineHost>(s: &mut S, now: f64, tx: &mut Tx) -> bool {
    let mut changed = false;
    changed |= turn_errors::poll(s, tx);
    changed |= working::sync(s);
    changed |= disclosure::poll(s, now, tx);
    changed |= timeline::projection_poll(s, tx);
    changed |= line_reveal::poll(s, now, tx);
    changed |= copied::poll(s, now, tx);
    changed |= diagram::poll(s, now, tx);
    changed |= images::poll(s, now, tx);
    changed |= poll_math(s);
    changed
}

/// True on this window's frame after background LaTeX renders finish, so the rows that drew them
/// (only those: `rune_kit::math::version`) swap from the code fallback to the rendered equation
/// without waiting for user input.
pub fn poll_math<S: TimelineHost>(s: &S) -> bool {
    s.math_client().is_some_and(rune_kit::math::Client::changed)
}

/// All ⌘⇧K knob groups owned by `session-view`.
pub const KNOB_GROUPS: &[rune_knobs::Group] = &[
    knobs::TIMELINE_GROUP,
    disclosure::GROUP,
    knobs::WORKING,
    compaction::GROUP,
    copied::GROUP,
];
