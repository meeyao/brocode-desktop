//! The engines as a library beside the app (`crates/office-library`), loaded when the first file
//! opens: `libopencode_office.dylib` in the bundle's `Frameworks`, `opencode_office.dll` beside
//! the exe, `libopencode_office.so` beside it or in `../lib/brocode`. Loaded once and
//! kept: scenes hold font and image bytes whose drop code is the library's.
//!
//! A library that is missing, cannot load or is another build's is a file that fails to open,
//! with a message that says which.

#![allow(unsafe_code)]

use std::alloc::Layout;
use std::ffi::{CString, c_char, c_void};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use office_preview::engine::{self, Api, Entry, Host, Job, Start, Worker};

static HOST: Host = Host { alloc, alloc_zeroed, dealloc, realloc };
static API: OnceLock<Result<&'static Api, String>> = OnceLock::new();

/// A document's thread in the library. Dropping it ends the thread after the job it is on.
pub struct Library {
    api: &'static Api,
    worker: *mut Worker,
}

// SAFETY: `worker` is the library's `Worker` (a channel's sender), used through `&` or dropped.
unsafe impl Send for Library {}

impl Library {
    /// Queues `job`; false when the thread has ended.
    pub fn send(&self, job: Job) -> bool {
        // SAFETY: `worker` is live until `drop`; the library takes the job's box.
        unsafe { (self.api.send)(self.worker, Box::into_raw(Box::new(job))) }
    }
}

impl Drop for Library {
    fn drop(&mut self) {
        // SAFETY: `start`'s answer, closed once.
        unsafe { (self.api.close)(self.worker) }
    }
}

/// Loads the library now; the error says what is wrong with it.
pub fn ready() -> Result<(), String> {
    API.get_or_init(load).clone().map(|_| ())
}

/// Starts the document's thread in the library, loading the library first when this is the
/// first file. Blocking (a first load after an install is checked by the system).
pub fn start(start: Start) -> Result<Library, String> {
    let api = API.get_or_init(load).clone()?;
    // SAFETY: the library is this build's (`load`); it takes the box.
    let worker = unsafe { (api.start)(Box::into_raw(Box::new(start))) };
    if worker.is_null() {
        return Err("The file could not be opened".to_owned());
    }
    Ok(Library { api, worker })
}

fn load() -> Result<&'static Api, String> {
    let path = path().ok_or_else(|| "The viewer for Word, Excel and PowerPoint files is missing from this install of Brocode. Install Brocode again to open them.".to_owned())?;
    let entry = open(&path).map_err(|e| format!("The viewer for Word, Excel and PowerPoint files could not be loaded ({}: {e}). Install Brocode again to open them.", path.display()))?;
    // SAFETY: `HOST` is a static; the entry reads `abi` during the call only and touches nothing
    // when it is not its own.
    let api = unsafe { entry(&HOST, &engine::abi()) };
    // SAFETY: a non-null answer is the library's static `Api`, and the library is never unloaded.
    unsafe { api.as_ref() }.ok_or_else(|| format!("The viewer for Word, Excel and PowerPoint files is from another version of Brocode ({}). Install Brocode again to open them.", path.display()))
}

fn path() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?;
    let places: &[&str] = if cfg!(target_os = "macos") {
        &["../Frameworks/libopencode_office.dylib", "libopencode_office.dylib"]
    } else if cfg!(windows) {
        &["opencode_office.dll"]
    } else {
        &["libopencode_office.so", "../lib/brocode/libopencode_office.so"]
    };
    places.iter().map(|place| dir.join(place)).find(|path| path.is_file())
}

#[cfg(unix)]
fn open(path: &Path) -> Result<Entry, String> {
    use std::ffi::{CStr, c_int};
    use std::os::unix::ffi::OsStrExt;

    unsafe extern "C" {
        fn dlopen(path: *const c_char, mode: c_int) -> *mut c_void;
        fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
        fn dlerror() -> *const c_char;
    }
    const RTLD_NOW: c_int = 2;
    const RTLD_LOCAL: c_int = if cfg!(target_vendor = "apple") { 4 } else { 0 };

    let name = CString::new(path.as_os_str().as_bytes()).map_err(|e| e.to_string())?;
    // SAFETY: a NUL-terminated path; the handle is kept for the life of the process.
    let handle = unsafe { dlopen(name.as_ptr(), RTLD_NOW | RTLD_LOCAL) };
    if handle.is_null() {
        // SAFETY: after a failed dlopen on this thread dlerror gives its message, or null.
        let message = unsafe { dlerror() };
        // SAFETY: a non-null dlerror answer is a C string.
        return Err(if message.is_null() { "it did not load".to_owned() } else { unsafe { CStr::from_ptr(message) }.to_string_lossy().into_owned() });
    }
    symbol(|name| {
        // SAFETY: a live handle and a NUL-terminated name.
        unsafe { dlsym(handle, name) }
    })
}

#[cfg(windows)]
fn open(path: &Path) -> Result<Entry, String> {
    use std::os::windows::ffi::OsStrExt;

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn LoadLibraryW(name: *const u16) -> *mut c_void;
        fn GetProcAddress(module: *mut c_void, name: *const c_char) -> *mut c_void;
    }

    let name: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    // SAFETY: a NUL-terminated full path; the module is kept for the life of the process.
    let module = unsafe { LoadLibraryW(name.as_ptr()) };
    if module.is_null() {
        return Err(std::io::Error::last_os_error().to_string());
    }
    symbol(|name| {
        // SAFETY: a live module and a NUL-terminated name.
        unsafe { GetProcAddress(module, name) }
    })
}

#[cfg(not(any(unix, windows)))]
fn open(_path: &Path) -> Result<Entry, String> {
    Err("this platform loads no libraries".to_owned())
}

#[cfg(any(unix, windows))]
fn symbol(find: impl Fn(*const c_char) -> *mut c_void) -> Result<Entry, String> {
    let name = CString::new(engine::ENTRY).map_err(|e| e.to_string())?;
    let entry = find(name.as_ptr());
    if entry.is_null() {
        return Err(format!("it has no {}", engine::ENTRY));
    }
    // SAFETY: the library exports this name with `Entry`'s signature (`crates/office-library`).
    Ok(unsafe { std::mem::transmute::<*mut c_void, Entry>(entry) })
}

unsafe extern "C" fn alloc(size: usize, align: usize) -> *mut u8 {
    // SAFETY: the layout the library's own caller gave its `GlobalAlloc`.
    unsafe { std::alloc::alloc(Layout::from_size_align_unchecked(size, align)) }
}

unsafe extern "C" fn alloc_zeroed(size: usize, align: usize) -> *mut u8 {
    // SAFETY: the layout the library's own caller gave its `GlobalAlloc`.
    unsafe { std::alloc::alloc_zeroed(Layout::from_size_align_unchecked(size, align)) }
}

unsafe extern "C" fn dealloc(ptr: *mut u8, size: usize, align: usize) {
    // SAFETY: a block this allocator gave, with the layout it was asked for.
    unsafe { std::alloc::dealloc(ptr, Layout::from_size_align_unchecked(size, align)) }
}

unsafe extern "C" fn realloc(ptr: *mut u8, size: usize, align: usize, new_size: usize) -> *mut u8 {
    // SAFETY: a block this allocator gave, with the layout it was asked for.
    unsafe { std::alloc::realloc(ptr, Layout::from_size_align_unchecked(size, align), new_size) }
}
