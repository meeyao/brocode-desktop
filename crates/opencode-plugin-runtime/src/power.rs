//! `Power`: keeps the display awake while any plugin holds a lock (Desktop's
//! `powerSaveBlocker.start("prevent-display-sleep")`). One blocker serves the whole process:
//! every window runs on the UI thread, where Windows' execution state lives; on macOS and Linux
//! a supervised `caffeinate -d` or `systemd-inhibit` runs while it's held. A lock goes with the
//! plugin half that took it.
// The OS call that keeps the display awake; the block says what it relies on.
#![allow(unsafe_code)]

use std::cell::RefCell;
use std::collections::BTreeSet;
use std::sync::atomic::{AtomicU64, Ordering};

use mlua::{Lua, Table};

use crate::luau::method;

/// The display blocker: released, or held by a set of locks.
#[derive(Debug, PartialEq, Eq)]
enum Blocker {
    Released,
    Held { locks: BTreeSet<u64> },
}

enum Event {
    Acquire(u64),
    Release(u64),
}

#[derive(Debug, PartialEq, Eq)]
enum Effect {
    Hold,
    Let,
}

impl Blocker {
    fn step(self, event: Event) -> (Blocker, Option<Effect>) {
        match (self, event) {
            (Blocker::Released, Event::Acquire(id)) => (Blocker::Held { locks: BTreeSet::from([id]) }, Some(Effect::Hold)),
            (Blocker::Held { mut locks }, Event::Acquire(id)) => {
                locks.insert(id);
                (Blocker::Held { locks }, None)
            }
            (Blocker::Held { mut locks }, Event::Release(id)) => {
                locks.remove(&id);
                if locks.is_empty() { (Blocker::Released, Some(Effect::Let)) } else { (Blocker::Held { locks }, None) }
            }
            (Blocker::Released, Event::Release(_)) => (Blocker::Released, None),
        }
    }
}

thread_local! {
    static BLOCKER: RefCell<Blocker> = const { RefCell::new(Blocker::Released) };
    #[cfg(all(not(target_os = "windows"), not(target_arch = "wasm32")))]
    static INHIBITOR: RefCell<Option<crate::process::Control>> = const { RefCell::new(None) };
}

static NEXT: AtomicU64 = AtomicU64::new(1);

fn dispatch(event: Event) {
    let effect = BLOCKER.with_borrow_mut(|b| {
        let (next, effect) = std::mem::replace(b, Blocker::Released).step(event);
        *b = next;
        effect
    });
    match effect {
        Some(Effect::Hold) => platform::hold(true),
        Some(Effect::Let) => platform::hold(false),
        None => {}
    }
}

/// Acquires a display-sleep lock and returns its id. Pass the id to [`release`] when done.
pub fn acquire() -> u64 {
    let id = NEXT.fetch_add(1, Ordering::Relaxed);
    dispatch(Event::Acquire(id));
    id
}

/// Lets lock `id` go (released, or its half went away).
pub fn release(id: u64) {
    dispatch(Event::Release(id));
}

/// Whether at least one display-sleep lock is currently held on this thread.
pub fn is_held() -> bool {
    BLOCKER.with_borrow(|b| matches!(b, Blocker::Held { .. }))
}

/// Populates the Luau `Power` service table (`Power:KeepAwake(reason?)`).
/// `on_acquire(id)` lets the host record ownership for reaping when the plugin half stops;
/// `on_release(id)` fires after [`release`] when Luau explicitly calls `lock:Release()`.
pub fn build(
    lua: &Lua,
    t: &Table,
    on_acquire: impl Fn(u64) + 'static,
    on_release: impl Fn(u64) + Clone + 'static,
) -> mlua::Result<()> {
    method(lua, t, "KeepAwake", move |lua, _reason: Option<String>| {
        let id = acquire();
        on_acquire(id);
        let lock = lua.create_table()?;
        let on_release = on_release.clone();
        method(lua, &lock, "Release", move |_, ()| {
            release(id);
            on_release(id);
            Ok(())
        })?;
        Ok(lock)
    })
}

#[cfg(target_os = "windows")]
mod platform {
    use windows_sys::Win32::System::Power::{ES_CONTINUOUS, ES_DISPLAY_REQUIRED, SetThreadExecutionState};

    /// The UI thread holds it; every window runs there.
    pub fn hold(on: bool) {
        // SAFETY: plain flags in, the previous state out.
        unsafe { SetThreadExecutionState(if on { ES_CONTINUOUS | ES_DISPLAY_REQUIRED } else { ES_CONTINUOUS }) };
    }
}

#[cfg(all(not(target_os = "windows"), not(target_arch = "wasm32")))]
mod platform {
    use crate::process::{Spec, launch};

    /// `caffeinate -d` (macOS) or `systemd-inhibit` (Linux), for as long as it's held.
    pub fn hold(on: bool) {
        super::INHIBITOR.with_borrow_mut(|held| {
            if let Some(p) = held.take() {
                p.kill();
            }
            if !on {
                return;
            }
            let (program, args): (&str, Vec<String>) = if cfg!(target_os = "macos") {
                ("caffeinate", vec!["-d".into(), "-w".into(), std::process::id().to_string()])
            } else {
                (
                    "systemd-inhibit",
                    ["--what=idle", "--who=Brocode", "--why=Keep screen active", "sleep", "infinity"]
                        .map(str::to_owned)
                        .to_vec(),
                )
            };
            let spec = Spec {
                program: program.into(),
                args,
                env: Vec::new(),
                cwd: String::new(),
                stdin: Vec::new(),
                keep_stdin: false,
                timeout: None,
                encoding: None,
                stderr_file: false,
            };
            *held = Some(launch(spec, |_| {}));
        });
    }
}

#[cfg(target_arch = "wasm32")]
mod platform {
    pub fn hold(_on: bool) {}
}
