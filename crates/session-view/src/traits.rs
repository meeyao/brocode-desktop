//! Composable capability views over [`TimelineHost`].
//!
//! Splits the full [`TimelineHost`] contract into four focused, opt-in capability traits so
//! components and helper modules can depend only on the slice of host functionality they need:
//!
//! - [`TimelineClock`] — animation knobs, frame clock, wall-clock time, and motion preferences.
//! - [`TranscriptSource`] — read-only access to active/tracked sessions, transcripts, and run status.
//! - [`TimelineStateAccess`] — shared and mutable access to per-window timeline [`ViewState`](crate::ViewState) fields.
//! - [`TimelineActions`] — user-triggered host actions (links, context menus, files, and saving tool output).
//!
//! Every type implementing [`TimelineHost`] automatically implements all four traits via blanket
//! implementations, keeping existing hosts (`apps/desktop`, embedder apps, `examples/read_only.rs`)
//! 100% source-compatible.

use std::path::{Path, PathBuf};

use opencode_kit::model::Transcript;
use rune::{PointerPress, Tx};
use rune_knobs::Knobs;

use crate::{TimelineHost, copied, diagram, disclosure, images, jump, line_reveal, working};

/// Animation knobs, frame clock, wall-clock time, and motion preferences for timeline rendering.
pub trait TimelineClock {
    fn knobs(&self) -> &Knobs;
    fn clock(&self) -> f64;
    fn display_hz(&self) -> Option<f64> {
        None
    }
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
}

impl<T: TimelineHost> TimelineClock for T {
    fn knobs(&self) -> &Knobs {
        TimelineHost::knobs(self)
    }

    fn clock(&self) -> f64 {
        TimelineHost::clock(self)
    }

    fn display_hz(&self) -> Option<f64> {
        TimelineHost::display_hz(self)
    }

    fn now_ms(&self) -> i64 {
        TimelineHost::now_ms(self)
    }

    fn reduced_motion(&self) -> bool {
        TimelineHost::reduced_motion(self)
    }

    fn smooth_timeline(&self) -> bool {
        TimelineHost::smooth_timeline(self)
    }
}

/// Read-only access to session transcripts, change versions, metadata, and run state.
pub trait TranscriptSource {
    fn active_session_id(&self) -> Option<&str>;
    fn transcript(&self, _session: &str) -> Option<&Transcript> {
        None
    }
    fn with_transcript<R>(&self, session: &str, f: impl FnOnce(&Transcript) -> R) -> Option<R> {
        Self::transcript(self, session).map(f)
    }
    /// A number that moves whenever session `session`'s transcript may have changed (a
    /// `Tracked` cell's version), so the timeline keeps its projection while it holds.
    fn transcript_version(&self, _session: &str) -> Option<u64> {
        None
    }
    /// Session `session`'s first message that may have changed since `transcript_version` read
    /// `since` (past the end when none did).
    fn transcript_changed_from(&self, _session: &str, _since: u64) -> Option<usize> {
        None
    }
    fn session_directory(&self, _session: &str) -> Option<&str> {
        None
    }
    fn session_title(&self, _session: &str) -> Option<String> {
        None
    }
    /// Hosts with a local outbox can exclude undelivered bubbles from run status.
    fn timeline_busy(&self, transcript: &Transcript) -> bool {
        transcript.busy()
    }
    fn is_session_live(&self, session: &str) -> bool {
        Self::with_transcript(self, session, Transcript::busy).unwrap_or(false)
    }
    fn is_session_stopped(&self, _session: &str) -> bool {
        false
    }
    /// The words in place of "Working" under a session that is still being started.
    fn working_label_for(&self, _session: &str) -> Option<&'static str> {
        None
    }
    /// What session `session` waits on the user for (a permission or a question answer).
    fn session_waiting(&self, _session: &str) -> Option<working::Waiting> {
        None
    }
    fn session_revert(&self, _session: &str) -> Option<&str> {
        None
    }
    fn full_codemode(&self, _session: &str) -> bool {
        false
    }
    fn for_each_tracked_session(&self, mut f: impl FnMut(&str)) {
        if let Some(id) = Self::active_session_id(self) {
            f(id);
        }
    }
}

impl<T: TimelineHost> TranscriptSource for T {
    fn active_session_id(&self) -> Option<&str> {
        TimelineHost::active_session_id(self)
    }

    fn transcript(&self, session: &str) -> Option<&Transcript> {
        TimelineHost::transcript(self, session)
    }

    fn with_transcript<R>(&self, session: &str, f: impl FnOnce(&Transcript) -> R) -> Option<R> {
        TimelineHost::with_transcript(self, session, f)
    }

    fn transcript_version(&self, session: &str) -> Option<u64> {
        TimelineHost::transcript_version(self, session)
    }

    fn transcript_changed_from(&self, session: &str, since: u64) -> Option<usize> {
        TimelineHost::transcript_changed_from(self, session, since)
    }

    fn session_directory(&self, session: &str) -> Option<&str> {
        TimelineHost::session_directory(self, session)
    }

    fn session_title(&self, session: &str) -> Option<String> {
        TimelineHost::session_title(self, session)
    }

    fn timeline_busy(&self, transcript: &Transcript) -> bool {
        TimelineHost::timeline_busy(self, transcript)
    }

    fn is_session_live(&self, session: &str) -> bool {
        TimelineHost::is_session_live(self, session)
    }

    fn full_codemode(&self, session: &str) -> bool {
        TimelineHost::full_codemode(self, session)
    }

    fn is_session_stopped(&self, session: &str) -> bool {
        TimelineHost::is_session_stopped(self, session)
    }

    fn working_label_for(&self, session: &str) -> Option<&'static str> {
        TimelineHost::working_label_for(self, session)
    }

    fn session_waiting(&self, session: &str) -> Option<working::Waiting> {
        TimelineHost::session_waiting(self, session)
    }

    fn session_revert(&self, session: &str) -> Option<&str> {
        TimelineHost::session_revert(self, session)
    }

    fn for_each_tracked_session(&self, f: impl FnMut(&str)) {
        TimelineHost::for_each_tracked_session(self, f);
    }
}

/// Shared and mutable access to per-window timeline UI state (matches [`ViewState`](crate::ViewState)).
pub trait TimelineStateAccess {
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
}

impl<T: TimelineHost> TimelineStateAccess for T {
    fn expanded(&self) -> &disclosure::Disclosures {
        TimelineHost::expanded(self)
    }

    fn expanded_mut(&mut self) -> &mut disclosure::Disclosures {
        TimelineHost::expanded_mut(self)
    }

    fn copied_state(&self) -> &copied::Copied {
        TimelineHost::copied_state(self)
    }

    fn copied_state_mut(&mut self) -> &mut copied::Copied {
        TimelineHost::copied_state_mut(self)
    }

    fn working_state(&self) -> &working::Working {
        TimelineHost::working_state(self)
    }

    fn working_state_mut(&mut self) -> &mut working::Working {
        TimelineHost::working_state_mut(self)
    }

    fn diagrams(&self) -> &diagram::Diagrams {
        TimelineHost::diagrams(self)
    }

    fn diagrams_mut(&mut self) -> &mut diagram::Diagrams {
        TimelineHost::diagrams_mut(self)
    }

    fn media_store(&self) -> &images::MediaStore {
        TimelineHost::media_store(self)
    }

    fn media_store_mut(&mut self) -> &mut images::MediaStore {
        TimelineHost::media_store_mut(self)
    }

    fn line_hold(&self) -> &line_reveal::Hold {
        TimelineHost::line_hold(self)
    }

    fn line_hold_mut(&mut self) -> &mut line_reveal::Hold {
        TimelineHost::line_hold_mut(self)
    }

    fn ends(&self) -> &jump::Ends {
        TimelineHost::ends(self)
    }

    fn ends_mut(&mut self) -> &mut jump::Ends {
        TimelineHost::ends_mut(self)
    }
}

/// Interactive host actions invoked from timeline rows, menus, and media viewers.
pub trait TimelineActions {
    fn open_link(&mut self, _url: &str, _tx: &mut Tx) {}
    /// A right-click in the timeline on `at`: a message, an image, or an image in a message.
    fn context_menu(&mut self, _at: images::menu::MenuAt, _press: PointerPress, _tx: &mut Tx) {}
    /// Opens the file at `path` in its default app (an image's Open in Preview).
    fn open_file(&mut self, path: PathBuf) {
        images::menu::open_with_system(&path);
    }
    /// Shows the file at `path` selected in the file manager; false when it couldn't.
    fn reveal_file(&mut self, path: &Path) -> bool {
        images::menu::reveal_with_system(path)
    }
    /// Saves a tool call's raw output bytes.
    fn save_tool_output(&mut self, _session: &str, _id: &str, _tx: &mut Tx) {}
}

impl<T: TimelineHost> TimelineActions for T {
    fn open_link(&mut self, url: &str, tx: &mut Tx) {
        TimelineHost::open_link(self, url, tx);
    }

    fn context_menu(&mut self, at: images::menu::MenuAt, press: PointerPress, tx: &mut Tx) {
        TimelineHost::context_menu(self, at, press, tx);
    }

    fn open_file(&mut self, path: PathBuf) {
        TimelineHost::open_file(self, path);
    }

    fn reveal_file(&mut self, path: &Path) -> bool {
        TimelineHost::reveal_file(self, path)
    }

    fn save_tool_output(&mut self, session: &str, id: &str, tx: &mut Tx) {
        TimelineHost::save_tool_output(self, session, id, tx);
    }
}
