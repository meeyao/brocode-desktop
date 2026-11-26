//! Brocode as a library, for a packaged Windows build: `opencode-desktop.exe` there is
//! the thin exe of `apps/desktop-thin`, which loads this library from beside itself and calls
//! [`opencode_desktop_run`]. Why, with the numbers: `apps/desktop-thin/src/main.rs`.
//!
//! It is `apps/desktop/src/main.rs` with an export in place of `main`: the same allocator over
//! the same app (`opencode_desktop`), built with the same profile, so the code a launch runs is
//! the code of the one-file exe that `cargo run` and the other platforms build.
#![cfg(windows)]

/// Counts each thread's allocations for the frame diagnostics and the benchmarks, over mimalloc
/// (as `apps/desktop/src/main.rs`).
#[global_allocator]
static ALLOC: procwatch::alloc::Counting<opencode_desktop::OomGuard<mimalloc::MiMalloc>> =
    procwatch::alloc::Counting::new(opencode_desktop::OomGuard(mimalloc::MiMalloc));

/// Runs the app on the calling thread, which must be the process's first; returns when the app
/// does (most exits end the process from inside).
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn opencode_desktop_run() -> i32 {
    opencode_desktop::run_native();
    0
}
