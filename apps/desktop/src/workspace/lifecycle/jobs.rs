//! Server requests that run off the main thread and land back on it: a job reads or writes
//! through the live server and hands back what to do with the answer, which the next poll
//! applies. A headless run asks inline, so a snapshot sees the answer on its first frame; it
//! never writes (see [`App::can_write`]) and skips delayed jobs (polls of a sign-in). The
//! harness's realtime probes (`--harness-writes`) ask as a window does.

use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::Duration;

use opencode_kit::client::Server;
use rune::Tx;

use crate::{App, Source};

/// What a finished job does to the app.
pub type Apply = Box<dyn FnOnce(&mut App, &mut Tx) + Send>;

pub struct Jobs {
    /// Picker reads belong to this server checkout; a new connection supersedes old replies.
    pub(crate) picker_reads: crate::connect::PickerReads,
    tx: Sender<Apply>,
    rx: Receiver<Apply>,
    /// Window-scoped deferred work (`later`, `deliverer`: plugin stores, server probes, dialogs),
    /// kept on the window rather than swapped or dropped with a server `Half`.
    window_tx: Sender<Apply>,
    window_rx: Receiver<Apply>,
}

impl Default for Jobs {
    fn default() -> Self {
        let (tx, rx) = channel();
        let (window_tx, window_rx) = channel();
        Jobs { tx, rx, window_tx, window_rx, picker_reads: Default::default() }
    }
}

impl Jobs {
    /// Trades only the server-scoped half (`picker_reads` and `job` replies), keeping `window_*` on
    /// the window across server checkouts and `401` `refit` drops.
    pub(crate) fn swap_server(&mut self, other: &mut Self) {
        std::mem::swap(&mut self.picker_reads, &mut other.picker_reads);
        std::mem::swap(&mut self.tx, &mut other.tx);
        std::mem::swap(&mut self.rx, &mut other.rx);
    }
}

impl App {
    /// Runs `work` against the live server; nothing in a fixture run.
    pub(crate) fn job(&self, work: impl FnOnce(&Server) -> Apply + Send + 'static) {
        self.job_after(Duration::ZERO, work);
    }

    /// [`App::job`] after `delay`, as a sign-in's next status poll.
    pub(crate) fn job_after(&self, delay: Duration, work: impl FnOnce(&Server) -> Apply + Send + 'static) {
        let Source::Live { server, .. } = &*self.source else { return };
        // The harness's realtime probes stand in for a window, so they ask off the main thread as
        // one does: an answer asked inline holds the frame clock while the server moves on.
        if crate::headless() && !crate::harness_writes() {
            if delay.is_zero() {
                self.jobs.tx.send(work(server)).ok();
            }
            return;
        }
        let (server, out, wake) = (server.clone(), self.jobs.tx.clone(), (*self.waker).clone());
        rune::task::spawn_after("opencode-job", delay, move || {
            out.send(work(&server)).ok();
            if let Some(w) = wake {
                w();
            }
        })
        .ok();
    }

    /// Applies `apply` on the next poll rather than now, so an answer known at once still lands
    /// after the transition that asked for it.
    pub(crate) fn later(&self, apply: Apply) {
        self.jobs.window_tx.send(apply).ok();
        if let Some(w) = &*self.waker {
            w();
        }
    }

    /// Hands what a worker thread finished back to the app's thread: the next poll applies it.
    pub(crate) fn deliverer(&self) -> impl Fn(Apply) + Send + 'static {
        let (out, wake) = (self.jobs.window_tx.clone(), (*self.waker).clone());
        move |apply| {
            out.send(apply).ok();
            if let Some(w) = &wake {
                w();
            }
        }
    }

    /// Whether this window may change the server: live, with a person at it, not read-only.
    pub(crate) fn can_write(&self) -> bool {
        matches!(*self.source, Source::Live { allow_send: true, .. }) && self.fixture_run.stage.is_none()
    }

    /// Lands finished server jobs on the checked-out server half (`servers::runtime::background`).
    pub(crate) fn poll_server_jobs(&mut self, tx: &mut Tx) -> bool {
        let done: Vec<Apply> = self.jobs.rx.try_iter().collect();
        let any = !done.is_empty();
        for apply in done {
            apply(self, tx);
        }
        let now = *self.clock;
        crate::connect::picker_reads::poll(self, now, tx);
        if self.can_write() {
            crate::connect::maybe_brand_prompt(self, tx);
        }
        any
    }

    /// Lands every finished window and active-server job.
    pub(crate) fn poll_jobs(&mut self, tx: &mut Tx) -> bool {
        let win_done: Vec<Apply> = self.jobs.window_rx.try_iter().collect();
        let mut any = !win_done.is_empty();
        for apply in win_done {
            apply(self, tx);
        }
        any |= self.poll_server_jobs(tx);
        any
    }
}
