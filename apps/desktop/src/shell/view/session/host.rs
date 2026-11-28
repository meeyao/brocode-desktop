//! The Desktop window as a host for the shared session timeline.

use opencode_kit::client::Request;
use opencode_kit::model::{Inbox, Transcript};
use rune::Tx;

use crate::pickers::PickerHost as _;
use crate::{
    App, Route, Source, agent_rows, background, copied, diagram, disclosure, jump, line_reveal,
    narrow, palette, pickers, plugin_host, popup, queue, quote, rewind, send_failed, settings,
    speech, subagents, tool_open, working,
};
use crate::app::CODE_WRAP;

impl session_view::TimelineHost for App {
    fn knobs(&self) -> &rune_knobs::Knobs {
        &self.diag.knobs
    }
    fn assistive(&self) -> bool {
        speech::listening(self)
    }
    fn clock(&self) -> f64 {
        *self.clock
    }
    fn display_hz(&self) -> Option<f64> {
        self.kernel.slots.untracked(&crate::app::DISPLAY_HZ).0.get()
    }
    fn now_ms(&self) -> i64 {
        *self.now_ms
    }
    fn reduced_motion(&self) -> bool {
        *self.appearance_state.reduced_motion
    }
    fn smooth_timeline(&self) -> bool {
        self.settings.smooth_timeline && !*self.appearance_state.reduced_motion
    }
    fn timeline_detail(&self) -> session_view::detail::Detail {
        self.settings.timeline_detail
    }
    fn code_block_lang(&self, language: &str) -> bool {
        plugin_host::surfaces::code_block_lang(self, language)
    }
    fn code_block(&self, p: &palette::Pal, language: &str, key: &str, code: &str, streaming: bool, version: u64, native: rune::El) -> Option<rune::El> {
        plugin_host::surfaces::code_block(self, p, language, key, code, streaming, version, native)
    }
    fn shell_output(&self, shell: &str) -> Option<&str> {
        self.kernel.slots.untracked(&crate::subagents::SHELLS).follow.output(shell)
    }

    fn output_save_failed(&mut self, reason: String) {
        crate::status_toast::fail(self, opencode_i18n::t!("common.requestFailed"), reason);
    }
    fn output_save_reporter(&mut self) -> Box<dyn FnOnce(String) + Send> {
        let (send, _) = self.kernel.slots.get_mut(&crate::app::OUTPUT_ERRORS).0.get_or_insert_with(std::sync::mpsc::channel);
        let (send, wake) = (send.clone(), self.waker.as_ref().cloned());
        Box::new(move |why| { if send.send(why).is_ok() && let Some(wake) = wake { wake(); } })
    }
    fn shell_output_version(&self) -> u64 {
        self.kernel.slots.untracked(&crate::subagents::SHELLS).follow.version()
    }
    fn turn_diffs(&self) -> bool {
        self.settings.turn_diffs
    }
    fn full_codemode(&self, session: &str) -> bool {
        crate::session::meta::flags::full_codemode(self, session)
    }
    fn wrap_code_blocks(&self) -> bool {
        self.settings.wrap_code_blocks
    }
    fn code_wrap_overrides(&self) -> Option<&std::collections::HashMap<String, bool>> {
        Some(self.kernel.slots.get(&CODE_WRAP))
    }
    fn set_code_wrap_override(&mut self, key: String, value: Option<bool>) {
        match value {
            Some(v) => { self.kernel.slots.get_mut(&CODE_WRAP).insert(key, v); }
            None => { self.kernel.slots.get_mut(&CODE_WRAP).remove(&key); }
        }
    }
    fn turn_diff(&self, session: &str, from: &str) -> session_view::turn_diff::State<'_> {
        self.turn_diffs.view(session, from)
    }
    fn retry_turn_diff(&mut self, session: &str, from: &str) {
        self.turn_diffs.retry(session, from);
    }
    fn lane_align(&self) -> session_view::jump::Align {
        match self.settings.background_activity {
            settings::BackgroundActivity::Center => session_view::jump::Align::Center,
            settings::BackgroundActivity::Right => session_view::jump::Align::End,
            settings::BackgroundActivity::Left | settings::BackgroundActivity::Off => session_view::jump::Align::Start,
        }
    }
    fn content_width(&self) -> f32 {
        narrow::content_width(self).unwrap_or_else(|| self.settings.timeline_width.width().content())
    }
    fn timeline_frame(&self) -> Option<&'static str> {
        Some("session")
    }
    fn active_session_id(&self) -> Option<&str> {
        match &*self.route {
            Route::Session { id, .. } => Some(id.as_str()),
            _ => None,
        }
    }
    fn transcript(&self, session: &str) -> Option<&Transcript> {
        rune::Tracked::untracked(&self.transcripts).get(session)
    }
    fn transcript_version(&self, session: &str) -> Option<u64> {
        Some(self.family_transcript_version(session).1)
    }
    fn transcript_changed_from(&self, session: &str, since: u64) -> Option<usize> {
        App::transcript_changed_from(self, session, since)
    }
    fn session_directory(&self, session: &str) -> Option<&str> {
        self.session(session).map(|i| i.directory.as_str())
    }
    fn session_title(&self, session: &str) -> Option<String> {
        self.mention_title(session)
    }
    fn is_session_live(&self, session: &str) -> bool {
        self.subtree_busy(session)
    }
    fn timeline_busy(&self, t: &Transcript) -> bool {
        // Unified busy definition (`App::subtree_busy` / `App::session_busy`): covers server
        // running set (with `/api/session/active` retry and SSE overlay), queued/steered/compaction
        // inbox work (`project-avatar-state.ts:98-114`), running or approval-waiting sub-agents,
        // and unsettled-turn fallback before `/api/session/active` syncs, while keeping settled
        // idle sessions idle (`message-timeline.tsx:468-483,549`, `turn-phase-machine.ts:57`).
        self.subtree_busy(&t.session)
    }
    fn working_label_for(&self, session: &str) -> Option<&'static str> {
        self.queue.starts.status(session)
    }
    fn is_session_stopped(&self, session: &str) -> bool {
        self.stops.has(session)
    }
    fn session_waiting(&self, session: &str) -> Option<session_view::working::Waiting> {
        let family = self.pending_for(session);
        if family.iter().any(|p| matches!(p, opencode_kit::model::Pending::Permission { .. })) {
            return Some(session_view::working::Waiting::Approval);
        }
        if family.iter().any(|p| matches!(p, opencode_kit::model::Pending::Form { .. })) {
            return Some(session_view::working::Waiting::Answer);
        }
        None
    }
    fn subagents_awaited(&self, session: &str) -> usize {
        // The capsule's count (`subagents::capsule`), once the session's own run is over.
        if self.session_busy(session) { 0 } else { self.running_subagents(session).len() }
    }
    fn session_revert(&self, session: &str) -> Option<&str> {
        self.session(session).and_then(|i| i.revert.as_deref())
    }
    fn for_each_tracked_session(&self, mut f: impl FnMut(&str)) {
        for id in self.transcripts.keys().chain(self.running.iter()) {
            f(id);
        }
    }
    fn expanded(&self) -> &disclosure::Disclosures {
        &self.timeline_fx.expanded
    }
    fn expanded_mut(&mut self) -> &mut disclosure::Disclosures {
        &mut self.timeline_fx.expanded
    }
    fn copied_state(&self) -> &copied::Copied {
        &self.timeline_fx.copied
    }
    fn copied_state_mut(&mut self) -> &mut copied::Copied {
        &mut self.timeline_fx.copied
    }
    fn working_state(&self) -> &working::Working {
        &self.timeline_fx.working
    }
    fn working_state_mut(&mut self) -> &mut working::Working {
        &mut self.timeline_fx.working
    }
    fn diagrams(&self) -> &diagram::Diagrams {
        &self.timeline_fx.diagram
    }
    fn diagrams_mut(&mut self) -> &mut diagram::Diagrams {
        &mut self.timeline_fx.diagram
    }
    fn media_store(&self) -> &session_view::images::MediaStore {
        &self.composer_ext.attach.store
    }
    fn media_store_mut(&mut self) -> &mut session_view::images::MediaStore {
        &mut self.composer_ext.attach.store
    }
    fn line_hold(&self) -> &line_reveal::Hold {
        &self.timeline_fx.line_hold
    }
    fn line_hold_mut(&mut self) -> &mut line_reveal::Hold {
        &mut self.timeline_fx.line_hold
    }
    fn ends(&self) -> &jump::Ends {
        &self.timeline_fx.ends
    }
    fn ends_mut(&mut self) -> &mut jump::Ends {
        &mut self.timeline_fx.ends
    }
    fn math_client(&self) -> Option<&rune_kit::math::Client> {
        Some(self.kernel.slots.get(&crate::app::MATH))
    }
    fn fold_user_messages(&self) -> bool {
        self.settings.fold_user
    }
    fn open_link(&mut self, url: &str, tx: &mut Tx) {
        self.open_reply_link(url, tx);
    }
    fn open_file(&mut self, path: std::path::PathBuf) {
        crate::open_in::reveal_abs(self, path.to_string_lossy().into_owned());
    }
    fn reveal_file(&mut self, path: &std::path::Path) -> bool {
        let Ok(verified) = browser_policy::verify_local_path(&path.to_string_lossy()) else {
            return false;
        };
        crate::open_in::reveal_abs(self, verified.path);
        true
    }
    fn reply_link_menu(&mut self, url: &str, press: rune::PointerPress, tx: &mut Tx) -> bool {
        crate::files::reply::menu(self, url, press, tx)
    }
    fn reply_video_source(&self, url: &str) -> Option<String> {
        crate::files::reply::video(self, url)
    }
    fn load_reply_video(&mut self, url: &str, _tx: &mut Tx) -> Option<String> {
        crate::files::reply::load_video(self, url)
    }
    fn context_menu(&mut self, at: session_view::images::menu::MenuAt, press: rune::PointerPress, tx: &mut Tx) {
        popup::context::open(self, at, press, tx);
    }
    fn code_links(&self, body: &str, live: bool) -> Option<std::rc::Rc<std::collections::HashSet<String>>> {
        self.reply_code_links(body, live)
    }
    fn code_links_version(&self) -> u64 {
        self.active_session_id().map_or(0, |sid| self.kernel.slots.untracked(&crate::files::FILES).reply_version(sid))
    }
    fn reply_link_help(url: &str) -> Option<String> {
        crate::files::link_tip::tip(url, "")
    }
    fn code_block_actions(p: &palette::Pal, language: Option<&str>, source: &str) -> Vec<rune::El> {
        plugin_host::code_block_actions(p, language, source)
    }
    fn send_request(&self, req: Request) {
        if let Source::Live { requests: Some(r), .. } = &*self.source {
            r.send(req).ok();
        }
    }
    /// Official `packages/app/src/session/composer/timeline-actions.ts:45`: child sessions hide
    /// the Switch model action on error cards.
    fn can_switch_model(&self) -> bool {
        self.subagent_page().is_none()
    }
    fn switch_model(&mut self, _tx: &mut Tx) {
        if self.composer_ext.pickers.open() != Some(pickers::Kind::Model) {
            self.open_picker(pickers::Kind::Model);
        }
    }
    fn focus_composer(&mut self, tx: &mut Tx) {
        App::focus_composer(self, tx);
    }
    fn model_name(&self, model: &opencode_kit::model::ModelRef) -> String {
        self.composer_ext.pickers.model_name(model)
    }
    fn model_speed_switch(&self, model: &opencode_kit::model::ModelRef, previous: &opencode_kit::model::ModelRef) -> Option<Option<String>> {
        self.composer_ext.pickers.speed_switch(model, previous)
    }
    fn tool_row_action(&self, p: &palette::Pal, session: &str, call: &session_view::tools::Call) -> Option<rune::El> {
        tool_open::action(self, p, session, call)
    }
    fn tool_row_action_version(&self, id: &str) -> u64 {
        u64::from(self.tool_step_showing(id))
    }
    fn can_open_subagent(&self) -> bool {
        true
    }
    fn resolve_subagent<'a>(&'a self, parent: &str, session: Option<&'a str>, title: &str) -> Option<&'a str> {
        subagents::child_of(self, parent, session, title)
    }
    fn subagent_run(&self, parent: &str, call: &str, child: Option<&str>, status: &opencode_kit::model::ToolStatus) -> agent_rows::Run {
        self.agent_run(parent, call, child, status)
    }
    fn prefetch_subagent(&self, child: &str) {
        self.prefetch_session(child);
    }
    fn subagent_hover(&self) -> Option<&str> {
        self.timeline_fx.agent_hover.as_deref()
    }
    fn set_subagent_hover(&mut self, call: Option<String>) {
        *self.timeline_fx.agent_hover = call;
    }
    fn open_subagent(&mut self, parent: &str, child: &str, title: &str, agent: &str, tx: &mut Tx) {
        self.open_child(parent, child, title, agent, tx);
    }
    fn can_open_session(&self) -> bool {
        true
    }
    fn open_session(&mut self, id: &str, tx: &mut Tx) {
        self.open_mentioned(id, tx);
    }
    fn session_ref(&self, id: &str) -> Option<session_view::session_chips::SessionRef> {
        self.session_ref_of(id)
    }
    fn session_chip_menu(&mut self, id: &str, press: rune::PointerPress, tx: &mut Tx) {
        self.open_session_chip_menu(id, press, tx);
    }
    fn loading_error(&self, session: &str) -> Option<&str> {
        self.opening.as_ref().filter(|o| o.id == session).and_then(|o| o.error())
    }
    fn retry_loading(&mut self, session: &str, tx: &mut Tx) {
        self.retry_open(session, tx);
    }
    fn partial_output(&self, session: &str) -> Option<&str> {
        self.opening.as_ref().filter(|o| o.id == session).and_then(|o| o.partial())
    }
    fn loading_older(&self, session: &str) -> bool {
        self.pages.loading_older(session)
    }
    fn more_above(&self, session: &str) -> bool {
        // The boot record's preview of a longer session: its older messages land above it once
        // its server answers, so it is drawn as it will stand under them from the first frame.
        self.pages.more_above(session) || self.launch.cold_transcript.as_ref().is_some_and(|cold| cold.id == session && cold.older)
    }
    fn watch_scroll_top(&self, session: &str) -> Option<Box<dyn Fn(rune::ScrollPos)>> {
        let (pages, nav) = (self.pages.watch_top(session), self.kernel.slots.get(&crate::message_nav::MESSAGE_NAV).watch());
        Some(Box::new(move |at| {
            pages(at);
            nav(at);
        }))
    }
    fn on_timeline_selection_change(s: &mut Self, change: &rune::SelectionChange, tx: &mut Tx) {
        quote::selection_changed(s, change, tx);
    }
    fn on_timeline_key(&mut self, ev: &rune::KeyEvent, tx: &mut Tx) -> bool {
        crate::keys::timeline_key(self, ev, tx)
    }
    fn find_row_version(&self, row_key: &str) -> u64 {
        self.kernel.slots.get(&crate::find::FIND).row_version(row_key)
    }
    fn find_text_highlights(&self, p: &palette::Pal, row_key: &str) -> Vec<rune::TextHighlight> {
        self.kernel.slots.get(&crate::find::FIND).text_highlights(p, row_key)
    }
    fn find_markdown_highlights(&self, p: &palette::Pal, row_key: &str) -> Vec<(&[std::ops::Range<usize>], rune_kit::Highlight)> {
        self.kernel.slots.get(&crate::find::FIND).markdown_highlights(p, row_key)
    }
    fn find_row_hits(&self, row_key: &str) -> Option<(&[Vec<std::ops::Range<usize>>], Option<usize>)> {
        self.kernel.slots.get(&crate::find::FIND).row_hits(row_key)
    }
    fn user_menu_open_on(&self, id: &str) -> bool {
        self.kernel.slots.get(&crate::rewind::REWIND).menu_open_on(id)
    }
    fn inline_quotes(&self) -> bool {
        // Enabled in every build channel (`opencode#66823`).
        true
    }
    fn quote_more_open(&self, id: &str) -> bool {
        self.kernel.slots.get(&crate::quote::QUOTE).more.contains(id)
    }
    fn open_quote_more(&mut self, id: String) {
        self.kernel.slots.get_mut(&crate::quote::QUOTE).more.insert(id);
    }
    fn user_menu(&self, p: &palette::Pal, session: &str, id: &str, _body: &str, pending: bool) -> Option<rune::El> {
        rewind::user_menu(self, p, session, id, pending)
    }
    fn user_status(&self, p: &palette::Pal, id: &str) -> Option<rune::El> {
        send_failed::footer(self, p, id)
    }
    fn user_status_version(&self, id: &str) -> u64 {
        send_failed::footer_version(self, id)
    }
    fn inbox_for(&self, session: &str) -> Option<&Inbox> {
        App::inbox_for(self, session)
    }
    fn queued_compaction(&self, session: &str) -> Option<&str> {
        self.inboxes.get(session).and_then(|i| i.compaction.as_deref())
    }
    fn pending_steer_label(&self, item: &opencode_kit::model::InboxItem) -> String {
        queue::pending_label(item)
    }
    fn turn_tail_extra(&self, p: &palette::Pal, session: &str, busy: bool) -> Option<rune::El> {
        background::hint(self, p, session, busy)
    }
}
