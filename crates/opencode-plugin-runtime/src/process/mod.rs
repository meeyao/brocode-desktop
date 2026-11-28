//! Processes plugins start (`Process:Spawn`, `Process:RunAsync`), supervised by the host: each
//! runs with its window hidden, is killed when its plugin stops or reloads, and on Windows sits
//! in a job object that kills it when the app exits however it does (a crash included).
//!
//! The app's thread only queues a spawn: a long-lived supervisor thread starts the process and
//! the threads that read its output and wait for it, so no thread starts on the app's thread
//! (on Windows that runs every library's thread-attach code under the loader lock). Output and
//! the exit go to the sink the Luau `Process` service passed, which wakes the window. A process
//! at rest costs the app's thread nothing.
//!
//! Each process's life is one explicit machine ([`Life`]) behind one lock that the app's thread
//! (a kill), the timeout thread and the supervisor's threads (start, exit) all step.
// Windows job objects and process handles; each block says what it relies on.
#![allow(unsafe_code)]

pub mod service;

use std::io::{Read, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{Sender, channel};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

/// What a plugin asked to start.
pub struct Spec {
    pub program: String,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
    pub cwd: String,
    pub stdin: Vec<u8>,
    pub keep_stdin: bool,
    pub timeout: Option<Duration>,
    /// How its output is encoded; `None` reads its first chunk to tell.
    pub encoding: Option<Encoding>,
    /// Stderr goes to a file, read back as it exits (Windows' OpenSSH never exits after writing
    /// to a stderr pipe once something failed).
    pub stderr_file: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Encoding {
    Utf8,
    Utf16Le,
}

/// Why a process that ended was stopped, when it was.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum End {
    TimedOut,
    Killed,
}

/// One process's life. A stop's reason sticks: the first of a timeout and a kill wins.
enum Life {
    /// Queued on the supervisor, not started yet.
    Queued,
    /// Stopped before it started: the supervisor never starts it.
    Cancelled(End),
    Running(Killer),
    /// Asked to stop; it runs until its waiter sees it go.
    Stopping(End),
    /// Gone, and why it was stopped if it was. Nothing is left to kill.
    Exited(Option<End>),
}

enum LifeEvent {
    Started(Killer),
    Stop(End),
    Exited,
}

/// What a step asks for, done under the lock so the handle can't be reused meanwhile: kill it
/// (stopped while running, or started after a stop).
enum LifeEffect {
    Terminate(Killer),
}

impl Life {
    fn step(self, event: LifeEvent) -> (Life, Option<LifeEffect>) {
        match (self, event) {
            (Life::Queued, LifeEvent::Started(k)) => (Life::Running(k), None),
            (Life::Cancelled(end), LifeEvent::Started(k)) => (Life::Stopping(end), Some(LifeEffect::Terminate(k))),
            (Life::Queued, LifeEvent::Stop(end)) => (Life::Cancelled(end), None),
            (Life::Running(k), LifeEvent::Stop(end)) => (Life::Stopping(end), Some(LifeEffect::Terminate(k))),
            (Life::Running(_), LifeEvent::Exited) => (Life::Exited(None), None),
            (Life::Stopping(end), LifeEvent::Exited) | (Life::Cancelled(end), LifeEvent::Exited) => (Life::Exited(Some(end)), None),
            (Life::Queued, LifeEvent::Exited) => (Life::Exited(None), None),
            (life, _) => (life, None),
        }
    }

    fn running(&self) -> bool {
        !matches!(self, Life::Exited(_) | Life::Cancelled(_))
    }

    fn end(&self) -> Option<End> {
        match self {
            Life::Cancelled(end) | Life::Stopping(end) | Life::Exited(Some(end)) => Some(*end),
            _ => None,
        }
    }
}

/// What the supervisor's threads and the app's thread share about one process.
struct Ctl {
    life: Mutex<Life>,
    stdin: Mutex<Option<ChildStdin>>,
}

impl Default for Ctl {
    fn default() -> Ctl {
        Ctl { life: Mutex::new(Life::Queued), stdin: Mutex::new(None) }
    }
}

/// Where a process's output and exit go.
type Sink = Arc<dyn Fn(Event) + Send + Sync>;

struct Job {
    spec: Spec,
    ctl: Arc<Ctl>,
    sink: Sink,
}

impl Job {
    fn send(&self, event: Event) {
        (self.sink)(event);
    }
}

/// A started process: write to it, kill it, ask whether it runs.
#[derive(Clone)]
pub struct Control(Arc<Ctl>);

impl Control {
    pub fn write(&self, bytes: &[u8], close: bool) -> bool {
        let Ok(mut stdin) = self.0.stdin.lock() else { return false };
        let ok = stdin.as_mut().is_some_and(|s| s.write_all(bytes).and_then(|()| s.flush()).is_ok());
        if close {
            *stdin = None;
        }
        ok
    }

    pub fn kill(&self) {
        self.0.step(LifeEvent::Stop(End::Killed));
    }

    /// It hasn't exited (queued counts: it will run).
    pub fn running(&self) -> bool {
        self.0.life.lock().is_ok_and(|l| l.running())
    }

    /// Why it was stopped, once a kill or its timeout stopped it.
    pub fn end(&self) -> Option<End> {
        self.0.end()
    }
}

/// What a process sends its sink.
pub enum Event {
    Output { stream: u8, text: String },
    Exit { code: Option<i64>, error: String },
}

/// Starts `spec` on the supervisor, as a plugin's process (window hidden, in the app's job);
/// `sink` hears its output and exit on the supervisor's threads.
pub fn launch(spec: Spec, sink: impl Fn(Event) + Send + Sync + 'static) -> Control {
    let ctl = Arc::new(Ctl::default());
    let sink: Sink = Arc::new(sink);
    let job = Job { spec, ctl: ctl.clone(), sink: sink.clone() };
    if !supervisor().lock().ok().is_some_and(|tx| tx.send(job).is_ok()) {
        sink(Event::Exit { code: None, error: "the process supervisor isn't running".into() });
    }
    Control(ctl)
}

fn supervisor() -> &'static Mutex<Sender<Job>> {
    static TX: OnceLock<Mutex<Sender<Job>>> = OnceLock::new();
    TX.get_or_init(|| {
        let (tx, rx) = channel::<Job>();
        let started = std::thread::Builder::new().name("plugin-processes".into()).spawn(move || {
            for job in rx {
                start(job);
            }
        });
        if let Err(e) = started {
            crate::tasks::log(format!("plugin process supervisor didn't start: {e}"));
        }
        Mutex::new(tx)
    })
}

/// Host-resolved command and base environment for a plugin's [`Spec`].
///
/// Returned by the resolver registered via [`set_resolver`]: `program` and `prefix_args` replace
/// `spec.program` (prefixing `spec.args`, e.g. for multi-token dev CLI commands or `.cmd` shims),
/// and `base_env`, when `Some`, replaces the inherited process environment before `spec.env`
/// overrides are applied.
pub struct Resolved {
    pub program: String,
    pub prefix_args: Vec<String>,
    pub base_env: Option<Vec<(String, String)>>,
}

type Resolver = Box<dyn Fn(&str) -> Option<Resolved> + Send + Sync>;

static RESOLVER: OnceLock<Resolver> = OnceLock::new();

/// Registers the host's process command and environment resolver, called on the supervisor thread
/// before each plugin process starts (e.g. mapping bare `"opencode"` to the app's bundled/staged
/// CLI and supplying the user's login-shell environment on Unix GUI launches).
pub fn set_resolver(resolver: impl Fn(&str) -> Option<Resolved> + Send + Sync + 'static) {
    let _ = RESOLVER.set(Box::new(resolver));
}

/// Starts the supervisor with the plugins, so the first spawn finds it running.
pub fn warm() {
    supervisor();
}

/// On the supervisor: starts `job`'s process and the threads that read and wait for it.
fn start(job: Job) {
    let spec = &job.spec;
    if job.ctl.life.lock().is_ok_and(|l| matches!(*l, Life::Cancelled(_))) {
        job.ctl.step(LifeEvent::Exited);
        return job.send(Event::Exit { code: None, error: String::new() });
    }
    let resolved = RESOLVER.get().and_then(|resolve| resolve(&spec.program));
    // Re-check cancellation in case the resolver waited (e.g. login-shell probe or CLI staging).
    if job.ctl.life.lock().is_ok_and(|l| matches!(*l, Life::Cancelled(_))) {
        job.ctl.step(LifeEvent::Exited);
        return job.send(Event::Exit { code: None, error: String::new() });
    }
    let (program, prefix_args, base_env) = match resolved {
        Some(r) => (r.program, r.prefix_args, r.base_env),
        None => (spec.program.clone(), Vec::new(), None),
    };
    let mut cmd = Command::new(&program);
    cmd.args(&prefix_args).args(&spec.args).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
    let log = spec.stderr_file.then(StderrFile::create).flatten();
    if let Some(file) = log.as_ref().and_then(|l| l.handle()) {
        cmd.stderr(file);
    }
    if let Some(base_env) = base_env {
        cmd.env_clear();
        for (k, v) in base_env {
            cmd.env(k, v);
        }
    }
    for (k, v) in &spec.env {
        cmd.env(k, v);
    }
    if !spec.cwd.is_empty() {
        cmd.current_dir(&spec.cwd);
    }
    platform::hide(&mut cmd);
    let mut child: Child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            job.ctl.step(LifeEvent::Exited);
            return job.send(Event::Exit { code: None, error: format!("{program}: {e}") });
        }
    };
    platform::adopt(&child);
    let mut stdin = child.stdin.take();
    if let Some(s) = stdin.as_mut()
        && !spec.stdin.is_empty()
    {
        s.write_all(&spec.stdin).and_then(|()| s.flush()).ok();
    }
    if spec.keep_stdin {
        if let Ok(mut slot) = job.ctl.stdin.lock() {
            *slot = stdin;
        }
    } else {
        drop(stdin);
    }
    job.ctl.step(LifeEvent::Started(Killer::of(&child)));
    let job = Arc::new(job);
    let (read_done, readers_done) = channel::<()>();
    let readers = [
        (1u8, child.stdout.take().map(|s| Box::new(s) as Box<dyn Read + Send>)),
        (2, child.stderr.take().map(|s| Box::new(s) as Box<dyn Read + Send>)),
    ]
    .into_iter()
    .filter_map(|(stream, pipe)| {
        let (pipe, job, done) = (pipe?, job.clone(), read_done.clone());
        std::thread::Builder::new()
            .name("plugin-process-read".into())
            .spawn(move || {
                read(pipe, stream, &job);
                done.send(()).ok();
            })
            .ok()
    })
    .count();
    drop(read_done);
    if let Some(after) = job.spec.timeout {
        let ctl = job.ctl.clone();
        std::thread::Builder::new()
            .name("plugin-process-timeout".into())
            .spawn(move || {
                std::thread::sleep(after);
                ctl.step(LifeEvent::Stop(End::TimedOut));
            })
            .ok();
    }
    let waiter = job.clone();
    let spawned = std::thread::Builder::new().name("plugin-process-wait".into()).spawn(move || {
        let status = child.wait();
        // Under the life lock: a kill from now on finds it exited, so it never reaches a handle
        // the system may have handed to another process once `child` drops.
        waiter.ctl.step(LifeEvent::Exited);
        drop(child);
        // Its output ends with it, unless a process it started holds its pipes open (`wsl.exe`
        // hands them to its service): then the exit doesn't wait on them.
        for _ in 0..readers {
            if readers_done.recv_timeout(Duration::from_millis(500)).is_err() {
                break;
            }
        }
        if let Ok(mut stdin) = waiter.ctl.stdin.lock() {
            *stdin = None;
        }
        if let Some(text) = log.map(|l| l.take(waiter.spec.encoding)).filter(|t| !t.is_empty()) {
            waiter.send(Event::Output { stream: 2, text });
        }
        let msg = match status {
            Ok(s) => Event::Exit { code: s.code().map(i64::from), error: String::new() },
            Err(e) => Event::Exit { code: None, error: e.to_string() },
        };
        waiter.send(msg);
    });
    if spawned.is_err() {
        job.ctl.step(LifeEvent::Stop(End::Killed));
    }
}

/// Where a process with `stderr_file` writes its stderr: a file of its own in the temp folder,
/// read back and removed as it exits.
struct StderrFile(std::path::PathBuf, Option<std::fs::File>);

impl StderrFile {
    fn create() -> Option<StderrFile> {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("opencode-stderr-{}-{n}.log", std::process::id()));
        let file = std::fs::File::create(&path).ok()?;
        Some(StderrFile(path, Some(file)))
    }

    fn handle(&self) -> Option<Stdio> {
        self.1.as_ref()?.try_clone().ok().map(Stdio::from)
    }

    fn take(self, encoding: Option<Encoding>) -> String {
        let bytes = std::fs::read(&self.0).unwrap_or_default();
        let mut decoder = Decoder::new(encoding);
        decoder.decode(&bytes) + &decoder.flush()
    }
}

impl Drop for StderrFile {
    fn drop(&mut self) {
        // Windows removes no file a handle still holds open.
        drop(self.1.take());
        std::fs::remove_file(&self.0).ok();
    }
}

/// Reads one pipe to its end, sending decoded chunks.
fn read(mut pipe: Box<dyn Read + Send>, stream: u8, job: &Job) {
    let mut buf = vec![0u8; 8192];
    let mut decoder = Decoder::new(job.spec.encoding);
    loop {
        match pipe.read(&mut buf) {
            Ok(0) | Err(_) => break,
            Ok(n) => {
                let text = decoder.decode(&buf[..n]);
                if !text.is_empty() {
                    job.send(Event::Output { stream, text });
                }
            }
        }
    }
    let rest = decoder.flush();
    if !rest.is_empty() {
        job.send(Event::Output { stream, text: rest });
    }
}

/// Desktop's output decoder (`runtime.ts`): a stream whose first chunk reads as UTF-16LE (a BOM,
/// or zero high bytes, as `wsl.exe` writes) stays UTF-16LE; anything else is UTF-8. Bytes split
/// across chunks carry over.
struct Decoder {
    utf16: Option<bool>,
    carry: Vec<u8>,
}

impl Decoder {
    /// A decoder for `encoding`, or one that tells from the first chunk.
    fn new(encoding: Option<Encoding>) -> Decoder {
        Decoder { utf16: encoding.map(|e| e == Encoding::Utf16Le), carry: Vec::new() }
    }

    fn decode(&mut self, chunk: &[u8]) -> String {
        let utf16 = *self.utf16.get_or_insert_with(|| utf16_like(chunk));
        let mut bytes = std::mem::take(&mut self.carry);
        bytes.extend_from_slice(chunk);
        if utf16 {
            let even = bytes.len() & !1;
            self.carry = bytes.split_off(even);
            let units: Vec<u16> = bytes.as_chunks::<2>().0.iter().map(|c| u16::from_le_bytes(*c)).collect();
            let text = String::from_utf16_lossy(&units);
            return text.strip_prefix('\u{feff}').map(str::to_owned).unwrap_or(text);
        }
        let valid = match std::str::from_utf8(&bytes) {
            Ok(_) => bytes.len(),
            Err(e) if e.error_len().is_none() => e.valid_up_to(),
            Err(_) => bytes.len(),
        };
        self.carry = bytes.split_off(valid);
        String::from_utf8_lossy(&bytes).into_owned()
    }

    fn flush(&mut self) -> String {
        let rest = std::mem::take(&mut self.carry);
        String::from_utf8_lossy(&rest).into_owned()
    }
}

fn utf16_like(chunk: &[u8]) -> bool {
    if chunk.starts_with(&[0xff, 0xfe]) {
        return true;
    }
    let pairs = chunk.len() / 2;
    if pairs < 2 {
        return false;
    }
    let odd = (0..pairs).filter(|i| chunk[i * 2 + 1] == 0).count();
    let even = (0..pairs).filter(|i| chunk[i * 2] == 0).count();
    odd >= pairs.div_ceil(3) && even * 2 <= odd
}

impl Ctl {
    /// Steps the process's life, and kills it under the lock when the step asks.
    fn step(&self, event: LifeEvent) {
        let Ok(mut life) = self.life.lock() else { return };
        let (next, effect) = std::mem::replace(&mut *life, Life::Queued).step(event);
        *life = next;
        if let Some(LifeEffect::Terminate(k)) = effect {
            k.kill();
        }
    }

    fn end(&self) -> Option<End> {
        self.life.lock().ok().and_then(|l| l.end())
    }
}

/// Kills a process the waiter thread owns.
#[derive(Clone, Copy)]
struct Killer {
    #[cfg(windows)]
    handle: isize,
    #[cfg(unix)]
    pid: i32,
}

impl Killer {
    fn of(child: &Child) -> Killer {
        #[cfg(windows)]
        {
            use std::os::windows::io::AsRawHandle;
            Killer { handle: child.as_raw_handle() as isize }
        }
        #[cfg(unix)]
        {
            Killer { pid: child.id() as i32 }
        }
        #[cfg(target_arch = "wasm32")]
        {
            let _ = child;
            Killer {}
        }
    }

    fn kill(&self) {
        #[cfg(windows)]
        // SAFETY: the waiter keeps the child, and so its handle, open while this is reachable.
        unsafe {
            windows_sys::Win32::System::Threading::TerminateProcess(self.handle as _, 1);
        }
        #[cfg(unix)]
        if self.pid > 0 {
            // SAFETY: `self.pid > 0` targets a single process (never 0 or a negative process group)
            // and `libc::kill` has no memory preconditions.
            unsafe {
                libc::kill(self.pid, libc::SIGTERM);
            }
        }
    }
}

/// A browser starts no processes: `Command::spawn` fails there, and nothing gets this far.
#[cfg(target_arch = "wasm32")]
mod platform {
    pub fn hide(_: &mut std::process::Command) {}
    pub fn adopt(_: &std::process::Child) {}
}

#[cfg(windows)]
mod platform {
    use std::os::windows::process::CommandExt;
    use std::process::{Child, Command};
    use std::sync::OnceLock;

    use windows_sys::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
        JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation, SetInformationJobObject,
    };

    /// No console window for a console program (`windowsHide`).
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    pub fn hide(cmd: &mut Command) {
        cmd.creation_flags(CREATE_NO_WINDOW);
    }

    /// The job every plugin process joins: closing it, as the app's exit does, kills them all.
    fn job() -> Option<isize> {
        static JOB: OnceLock<Option<isize>> = OnceLock::new();
        *JOB.get_or_init(|| {
            // SAFETY: plain Win32 calls with valid, zeroed arguments; the handle stays open for the
            // app's life on purpose.
            unsafe {
                let job = CreateJobObjectW(std::ptr::null(), std::ptr::null());
                if job.is_null() {
                    return None;
                }
                let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
                info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
                let size = size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32;
                (SetInformationJobObject(job, JobObjectExtendedLimitInformation, (&raw const info).cast(), size) != 0).then_some(job as isize)
            }
        })
    }

    pub fn adopt(child: &Child) {
        use std::os::windows::io::AsRawHandle;
        if let Some(job) = job() {
            // SAFETY: both handles are open; a process already in a job that forbids nesting just
            // stays out of ours.
            unsafe { AssignProcessToJobObject(job as _, child.as_raw_handle() as _) };
        }
    }
}

#[cfg(unix)]
mod platform {
    use std::process::{Child, Command};

    pub fn hide(_: &mut Command) {}

    pub fn adopt(_: &Child) {}
}
