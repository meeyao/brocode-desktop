//! `brocode-setup.exe`: Brocode's Windows setup. One click installs the app for
//! the user, with no administrator, where the updater expects it (`win_install`):
//!
//! 1. Install (with "Create a desktop shortcut") on the first page.
//! 2. Copies of the app running from the install are asked to close.
//! 3. The embedded release zip (the archive the updater downloads) unpacks beside the install
//!    folder and takes its place with a rename, the way the updater swaps versions, so a failed
//!    install leaves the previous version as it was.
//! 4. The Start menu shortcut (and the desktop one), the Apps & features entry, the app's URL
//!    scheme (`brocode://`, never `opencode://`) and the toasts' name and icon are
//!    registered (`win_install::register`).
//! 5. A process is made from the installed exe and ended before it runs, while the last page
//!    shows: Windows Defender's check of the new file is over before the person's own first
//!    start (`install::first_start`).
//! 6. "Launch Brocode" on the last page starts it.
//!
//! The browser engine isn't in the setup: the app downloads it in the background once it's idle
//! (`browser_pane::prefetch`), into the engine folder every Brocode build shares.
//!
//! ```text
//! brocode-setup.exe [--silent | /S] [--desktop-shortcut] [--launch] [--dir <install folder>] [--log <file>]
//! ```
//!
//! `--silent` installs without a window, with the desktop shortcut and launch only when asked.
//! `--dir` installs somewhere else (a test's scratch folder); the updater and the uninstaller
//! follow the install wherever it is.
//!
//! Release builds are Authenticode-signed (the release workflow's `sign-windows` and
//! `sign-windows-setup`): the setup, and the install folder in its payload before it's zipped.
#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

#[cfg(target_os = "windows")]
mod install;
#[cfg(target_os = "windows")]
mod pages;

fn main() {
    #[cfg(target_os = "windows")]
    std::process::exit(install::run());
    #[cfg(not(target_os = "windows"))]
    eprintln!("brocode-setup installs Brocode on Windows; other systems use their own packages.");
}
