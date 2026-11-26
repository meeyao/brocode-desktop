//! The exe of a packaged Windows build: it loads the app, a library beside it, and runs it.
//!
//! Why the app is not in the exe. Windows Defender checks an exe it has not seen inside
//! `CreateProcess`, before the process exists, on its own CPU: about 0.5 s plus 30-36 ms per MiB
//! of the file (6,451 ms for the signed 163 MiB exe of dev-479; 5,157-6,939 ms for a 160 MiB
//! stand-in, 472-613 ms for one of 64 KiB). Every new build is a file it has not seen, so a
//! person who unpacks a build and starts it waited that long for the process to exist. Windows
//! does not check a library that way: a never-seen 160 MiB library loads in 137-220 ms with none
//! of Defender's CPU. A small exe that loads a 160 MiB library ran to its exit in 544-1,613 ms
//! where one 160 MiB exe took 5,563-6,987 ms (a folder Defender scans, new files every trial; the
//! pull request that added this file has the table).
//!
//! This exe is the same bytes in every build of a channel (`build.rs`), so Defender has seen a
//! new build's exe already on a machine that ran an earlier one: 25-48 ms where a never-seen
//! small exe took 391-1,654 ms.
//!
//! The library is this file's own path with `.dll` for `.exe`: `opencode-desktop.dll` beside
//! `opencode-desktop.exe` in an install, and the same for a copy under another name (the
//! uninstaller's in the temp folder, which the app copies with its library).
//!
//! An update's helper is a hard link to the staged version's exe in the updates folder
//! (`features/updater/src/windows.rs` `hand_off`), started with `--apply-update ... --staged
//! <folder>`, and no library is beside it: every version before this one links the exe alone. So
//! the exe links the staged folder's library beside itself first (a copy across volumes), as the
//! updater links the exe: the swap renames the staged folder, which a library loaded from inside
//! it would hold.

fn main() {
    #[cfg(windows)]
    std::process::exit(thin::run());
}

#[cfg(windows)]
#[allow(unsafe_code)]
mod thin {
    use std::ffi::c_void;
    use std::os::windows::ffi::OsStrExt as _;
    use std::path::{Path, PathBuf};

    /// The library's name in an install folder (`apps/installer/scripts/package.ps1`).
    const LIBRARY: &str = "opencode-desktop.dll";
    /// The library's one export (`apps/desktop-library/src/lib.rs`).
    const ENTRY: &[u8] = b"opencode_desktop_run\0";
    const STD_ERROR_HANDLE: u32 = -12i32 as u32;
    const MB_ICONERROR: u32 = 0x10;

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn LoadLibraryW(name: *const u16) -> *mut c_void;
        fn GetProcAddress(module: *mut c_void, name: *const u8) -> *mut c_void;
        fn GetStdHandle(which: u32) -> *mut c_void;
    }

    fn wide(text: &std::ffi::OsStr) -> Vec<u16> {
        text.encode_wide().chain([0]).collect()
    }

    fn load(path: &Path) -> Option<*mut c_void> {
        // SAFETY: a NUL-terminated path. The library stays loaded for the life of the process.
        let module = unsafe { LoadLibraryW(wide(path.as_os_str()).as_ptr()) };
        (!module.is_null()).then_some(module)
    }

    pub fn run() -> i32 {
        let Ok(exe) = std::env::current_exe() else { return fail("Windows did not say where this program is.") };
        let library = exe.with_extension("dll");
        let module = load(&library).or_else(|| {
            link_staged(&library)?;
            load(&library)
        });
        let Some(module) = module else {
            return fail(&format!("{} is missing or could not be loaded ({}). Install the app again.", library.display(), std::io::Error::last_os_error()));
        };
        // SAFETY: `module` is loaded and the name is NUL-terminated.
        let entry = unsafe { GetProcAddress(module, ENTRY.as_ptr()) };
        if entry.is_null() {
            return fail(&format!("{} is not this app's library. Install the app again.", library.display()));
        }
        // SAFETY: the export is `extern "C" fn() -> i32` (apps/desktop-library/src/lib.rs), found by its
        // name in the library this exe was packaged with.
        let entry: extern "C" fn() -> i32 = unsafe { std::mem::transmute(entry) };
        entry()
    }

    /// An update's helper: the staged folder's library, linked (or copied) to `library`.
    fn link_staged(library: &Path) -> Option<()> {
        let mut args = std::env::args_os().skip(1);
        if args.next()? != "--apply-update" {
            return None;
        }
        // The helper's own arguments end at `--`; the app's follow it.
        let mut own = args.take_while(|a| a != "--");
        own.find(|a| a == "--staged")?;
        let staged = PathBuf::from(own.next()?).join(LIBRARY);
        std::fs::remove_file(library).ok();
        std::fs::hard_link(&staged, library).or_else(|_| std::fs::copy(&staged, library).map(drop)).ok()
    }

    /// Says why the app can't start: on the error stream when the starter gave one (a terminal, a
    /// script), in a message box when a person started it from the shell.
    fn fail(message: &str) -> i32 {
        // SAFETY: asks for this process's own handle.
        let stream = unsafe { GetStdHandle(STD_ERROR_HANDLE) };
        if !stream.is_null() && stream as isize != -1 {
            eprintln!("opencode-desktop: {message}");
            return 1;
        }
        // SAFETY: user32 by its system name, and `MessageBoxW` by its name and documented
        // signature; both strings are NUL-terminated and live through the call.
        unsafe {
            let user32 = LoadLibraryW(wide("user32.dll".as_ref()).as_ptr());
            let show = if user32.is_null() { std::ptr::null_mut() } else { GetProcAddress(user32, c"MessageBoxW".as_ptr().cast()) };
            if !show.is_null() {
                let show: extern "system" fn(*mut c_void, *const u16, *const u16, u32) -> i32 = std::mem::transmute(show);
                show(std::ptr::null_mut(), wide(message.as_ref()).as_ptr(), wide("Brocode".as_ref()).as_ptr(), MB_ICONERROR);
            }
        }
        1
    }
}
