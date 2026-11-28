//! The few EGL, GLES and GBM calls the frame copy needs, loaded at run time from the system's
//! libraries: an engine on a machine without them still runs, with BGRA frames.
#![allow(unsafe_code, non_snake_case)]

use std::ffi::{CStr, CString, c_char, c_void};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};

pub type Display = *mut c_void;
pub type Context = *mut c_void;
pub type Image = *mut c_void;
pub type Sync = *mut c_void;
pub type Bo = *mut c_void;

pub const NONE: i32 = 0x3038;
const EXTENSIONS: i32 = 0x3055;
const OPENGL_ES_API: u32 = 0x30A0;
const CONTEXT_MAJOR_VERSION: i32 = 0x3098;
const PLATFORM_GBM: u32 = 0x31D7;
pub const LINUX_DMA_BUF: u32 = 0x3270;
pub const WIDTH: i32 = 0x3057;
pub const HEIGHT: i32 = 0x3056;
pub const LINUX_DRM_FOURCC: i32 = 0x3271;
pub const DMA_BUF_PLANE0_FD: i32 = 0x3272;
pub const DMA_BUF_PLANE0_OFFSET: i32 = 0x3273;
pub const DMA_BUF_PLANE0_PITCH: i32 = 0x3274;
pub const DMA_BUF_PLANE0_MODIFIER_LO: i32 = 0x3443;
pub const DMA_BUF_PLANE0_MODIFIER_HI: i32 = 0x3444;
const SYNC_FENCE: u32 = 0x30F9;
const SYNC_FLUSH_COMMANDS_BIT: i32 = 0x0001;
const CONDITION_SATISFIED: i32 = 0x30F6;
const ALREADY_SIGNALED: i32 = 0x30F5;

pub const TEXTURE_2D: u32 = 0x0DE1;
pub const READ_FRAMEBUFFER: u32 = 0x8CA8;
pub const DRAW_FRAMEBUFFER: u32 = 0x8CA9;
pub const COLOR_ATTACHMENT0: u32 = 0x8CE0;
pub const FRAMEBUFFER_COMPLETE: u32 = 0x8CD5;
pub const COLOR_BUFFER_BIT: u32 = 0x4000;
pub const NEAREST: u32 = 0x2600;
const GL_EXTENSIONS: u32 = 0x1F03;
pub const BGRA_EXT: u32 = 0x80E1;
pub const RGBA: u32 = 0x1908;
pub const UNSIGNED_BYTE: u32 = 0x1401;
pub const NO_ERROR: u32 = 0;

const GBM_BO_USE_RENDERING: u32 = 1 << 2;

struct Lib(*mut c_void);

impl Lib {
    fn open(name: &str) -> Result<Self, String> {
        let c = CString::new(name).unwrap();
        // SAFETY: a plain dlopen of a system library by soname.
        let handle = unsafe { libc::dlopen(c.as_ptr(), libc::RTLD_NOW | libc::RTLD_LOCAL) };
        if handle.is_null() { Err(format!("{name} isn't installed")) } else { Ok(Self(handle)) }
    }

    fn sym(&self, name: &str) -> *mut c_void {
        let c = CString::new(name).unwrap();
        // SAFETY: looks a symbol up in a library this holds open.
        unsafe { libc::dlsym(self.0, c.as_ptr()) }
    }
}

macro_rules! api {
    ($($field:ident: $name:literal => fn($($arg:ty),*) $(-> $ret:ty)?;)*) => {
        /// Function pointers into the loaded libraries, which stay open while this lives.
        pub struct Api {
            _libs: [Lib; 3],
            $(pub $field: unsafe extern "C" fn($($arg),*) $(-> $ret)?,)*
        }

        impl Api {
            pub fn load() -> Result<Self, String> {
                let libs = [Lib::open("libEGL.so.1")?, Lib::open("libGLESv2.so.2")?, Lib::open("libgbm.so.1")?];
                let get: unsafe extern "C" fn(*const c_char) -> *mut c_void = {
                    let p = libs[0].sym("eglGetProcAddress");
                    if p.is_null() {
                        return Err("libEGL has no eglGetProcAddress".into());
                    }
                    // SAFETY: eglGetProcAddress has this signature.
                    unsafe { std::mem::transmute(p) }
                };
                let find = |name: &str| -> Result<*mut c_void, String> {
                    let direct = libs.iter().map(|l| l.sym(name)).find(|p| !p.is_null());
                    let c = CString::new(name).unwrap();
                    // SAFETY: a lookup by name; extension functions come from here.
                    let p = direct.unwrap_or_else(|| unsafe { get(c.as_ptr()) });
                    if p.is_null() { Err(format!("{name} is missing")) } else { Ok(p) }
                };
                Ok(Self {
                    // SAFETY: each symbol has the signature its field declares (EGL 1.4, GLES 3.0,
                    // EGL_KHR_image_base, EGL_KHR_fence_sync, GL_OES_EGL_image, GBM).
                    $($field: unsafe { std::mem::transmute::<*mut c_void, unsafe extern "C" fn($($arg),*) $(-> $ret)?>(find($name)?) },)*
                    _libs: libs,
                })
            }
        }
    };
}

api! {
    get_platform_display: "eglGetPlatformDisplayEXT" => fn(u32, *mut c_void, *const i32) -> Display;
    initialize: "eglInitialize" => fn(Display, *mut i32, *mut i32) -> u32;
    terminate: "eglTerminate" => fn(Display) -> u32;
    query_string: "eglQueryString" => fn(Display, i32) -> *const c_char;
    bind_api: "eglBindAPI" => fn(u32) -> u32;
    create_context: "eglCreateContext" => fn(Display, *mut c_void, Context, *const i32) -> Context;
    destroy_context: "eglDestroyContext" => fn(Display, Context) -> u32;
    make_current: "eglMakeCurrent" => fn(Display, *mut c_void, *mut c_void, Context) -> u32;
    create_image: "eglCreateImageKHR" => fn(Display, Context, u32, *mut c_void, *const i32) -> Image;
    destroy_image: "eglDestroyImageKHR" => fn(Display, Image) -> u32;
    create_sync: "eglCreateSyncKHR" => fn(Display, u32, *const i32) -> Sync;
    client_wait_sync: "eglClientWaitSyncKHR" => fn(Display, Sync, i32, u64) -> i32;
    destroy_sync: "eglDestroySyncKHR" => fn(Display, Sync) -> u32;
    image_target_texture: "glEGLImageTargetTexture2DOES" => fn(u32, Image);
    gen_textures: "glGenTextures" => fn(i32, *mut u32);
    delete_textures: "glDeleteTextures" => fn(i32, *const u32);
    bind_texture: "glBindTexture" => fn(u32, u32);
    gen_framebuffers: "glGenFramebuffers" => fn(i32, *mut u32);
    delete_framebuffers: "glDeleteFramebuffers" => fn(i32, *const u32);
    bind_framebuffer: "glBindFramebuffer" => fn(u32, u32);
    framebuffer_texture_2d: "glFramebufferTexture2D" => fn(u32, u32, u32, u32, i32);
    check_framebuffer_status: "glCheckFramebufferStatus" => fn(u32) -> u32;
    blit_framebuffer: "glBlitFramebuffer" => fn(i32, i32, i32, i32, i32, i32, i32, i32, u32, u32);
    flush: "glFlush" => fn();
    read_pixels: "glReadPixels" => fn(i32, i32, i32, i32, u32, u32, *mut c_void);
    get_error: "glGetError" => fn() -> u32;
    get_string: "glGetString" => fn(u32) -> *const c_char;
    gbm_create_device: "gbm_create_device" => fn(i32) -> *mut c_void;
    gbm_device_destroy: "gbm_device_destroy" => fn(*mut c_void);
    gbm_bo_create_with_modifiers2: "gbm_bo_create_with_modifiers2" => fn(*mut c_void, u32, u32, u32, *const u64, u32, u32) -> Bo;
    gbm_bo_get_fd: "gbm_bo_get_fd" => fn(Bo) -> i32;
    gbm_bo_get_stride: "gbm_bo_get_stride" => fn(Bo) -> u32;
    gbm_bo_get_offset: "gbm_bo_get_offset" => fn(Bo, i32) -> u32;
    gbm_bo_get_modifier: "gbm_bo_get_modifier" => fn(Bo) -> u64;
    gbm_bo_get_plane_count: "gbm_bo_get_plane_count" => fn(Bo) -> i32;
    gbm_bo_destroy: "gbm_bo_destroy" => fn(Bo);
}

/// One dmabuf plane; `modifier` None for the producer's implicit layout.
pub struct Plane {
    pub fd: i32,
    pub stride: u32,
    pub offset: u32,
    pub modifier: Option<u64>,
}

/// An EGL context with no surface on one render node, through GBM.
pub struct Egl {
    pub api: Api,
    pub display: Display,
    context: Context,
    gbm: *mut c_void,
    _node: OwnedFd,
}

// SAFETY: the context is made current on whichever thread copies, one at a time (the engine's
// main thread; it's made on another and released there first).
unsafe impl Send for Egl {}
unsafe impl std::marker::Sync for Egl {}

impl Egl {
    pub fn open(node: &str) -> Result<Self, String> {
        let api = Api::load()?;
        let path = CString::new(node).map_err(|_| "bad render node path")?;
        // SAFETY: opens the render node; the fd is owned from here.
        let raw = unsafe { libc::open(path.as_ptr(), libc::O_RDWR | libc::O_CLOEXEC) };
        if raw < 0 {
            return Err(format!("open {node}: {}", std::io::Error::last_os_error()));
        }
        // SAFETY: `raw` was just opened and nothing else owns it.
        let node_fd = unsafe { OwnedFd::from_raw_fd(raw) };
        // SAFETY: the calls below follow EGL's and GBM's documented order on handles made here;
        // each failure path frees what was made before it.
        unsafe {
            let gbm = (api.gbm_create_device)(node_fd.as_raw_fd());
            if gbm.is_null() {
                return Err(format!("no GBM device on {node}"));
            }
            let display = (api.get_platform_display)(PLATFORM_GBM, gbm, std::ptr::null());
            if display.is_null() || (api.initialize)(display, std::ptr::null_mut(), std::ptr::null_mut()) == 0 {
                (api.gbm_device_destroy)(gbm);
                return Err(format!("no EGL display on {node}"));
            }
            let listed = (api.query_string)(display, EXTENSIONS);
            let extensions = if listed.is_null() { String::new() } else { CStr::from_ptr(listed).to_string_lossy().into_owned() };
            let needed = ["EGL_EXT_image_dma_buf_import_modifiers", "EGL_KHR_fence_sync", "EGL_KHR_surfaceless_context", "EGL_KHR_no_config_context"];
            if let Some(missing) = needed.iter().find(|e| !extensions.split(' ').any(|x| x == **e)) {
                (api.terminate)(display);
                (api.gbm_device_destroy)(gbm);
                return Err(format!("EGL on {node} lacks {missing}"));
            }
            (api.bind_api)(OPENGL_ES_API);
            let attribs = [CONTEXT_MAJOR_VERSION, 3, NONE];
            let context = (api.create_context)(display, std::ptr::null_mut(), std::ptr::null_mut(), attribs.as_ptr());
            if context.is_null() {
                (api.terminate)(display);
                (api.gbm_device_destroy)(gbm);
                return Err(format!("no GLES 3 context on {node}"));
            }
            Ok(Self { api, display, context, gbm, _node: node_fd })
        }
    }

    pub fn make_current(&self) -> bool {
        // SAFETY: the context is live; no surfaces (EGL_KHR_surfaceless_context).
        unsafe { (self.api.make_current)(self.display, std::ptr::null_mut(), std::ptr::null_mut(), self.context) != 0 }
    }

    /// Whether the current context reads pixels back as BGRA (`GL_EXT_read_format_bgra`).
    pub fn reads_bgra(&self) -> bool {
        // SAFETY: a query on the current context; a null answer is no extensions.
        let listed = unsafe { (self.api.get_string)(GL_EXTENSIONS) };
        // SAFETY: a non-null answer is a NUL-terminated string GL owns.
        !listed.is_null() && unsafe { CStr::from_ptr(listed) }.to_string_lossy().split(' ').any(|e| e == "GL_EXT_read_format_bgra")
    }

    pub fn release(&self) {
        // SAFETY: releases this thread's current context.
        unsafe { (self.api.make_current)(self.display, std::ptr::null_mut(), std::ptr::null_mut(), std::ptr::null_mut()) };
    }

    /// A linear `width` × `height` ARGB8888 buffer the GPU can render to.
    pub fn linear_bo(&self, width: u32, height: u32, fourcc: u32) -> Bo {
        let linear = 0u64;
        // SAFETY: the GBM device is live; one modifier.
        unsafe { (self.api.gbm_bo_create_with_modifiers2)(self.gbm, width, height, fourcc, &linear, 1, GBM_BO_USE_RENDERING) }
    }

    /// Waits until the GPU has run everything submitted so far. False if it failed or took over
    /// a second.
    pub fn finish(&self) -> bool {
        // SAFETY: a fence on the current context, waited on and destroyed here.
        unsafe {
            let sync = (self.api.create_sync)(self.display, SYNC_FENCE, [NONE].as_ptr());
            if sync.is_null() {
                return false;
            }
            (self.api.flush)();
            let waited = (self.api.client_wait_sync)(self.display, sync, SYNC_FLUSH_COMMANDS_BIT, 1_000_000_000);
            (self.api.destroy_sync)(self.display, sync);
            waited == CONDITION_SATISFIED || waited == ALREADY_SIGNALED
        }
    }

    /// An EGL image over a single-plane dmabuf.
    pub fn import(&self, plane: Plane, width: u32, height: u32, fourcc: u32) -> Image {
        let Plane { fd, stride, offset, modifier } = plane;
        let mut attribs = vec![
            WIDTH, width as i32, HEIGHT, height as i32, LINUX_DRM_FOURCC, fourcc as i32,
            DMA_BUF_PLANE0_FD, fd, DMA_BUF_PLANE0_OFFSET, offset as i32, DMA_BUF_PLANE0_PITCH, stride as i32,
        ];
        if let Some(m) = modifier {
            attribs.extend([DMA_BUF_PLANE0_MODIFIER_LO, m as u32 as i32, DMA_BUF_PLANE0_MODIFIER_HI, (m >> 32) as u32 as i32]);
        }
        attribs.push(NONE);
        // SAFETY: `fd` is a dmabuf the caller holds open for this call; EGL dups what it keeps.
        unsafe { (self.api.create_image)(self.display, std::ptr::null_mut(), LINUX_DMA_BUF, std::ptr::null_mut(), attribs.as_ptr()) }
    }
}

impl Drop for Egl {
    fn drop(&mut self) {
        // SAFETY: the handles were made in `open` and nothing uses them after this.
        unsafe {
            (self.api.make_current)(self.display, std::ptr::null_mut(), std::ptr::null_mut(), std::ptr::null_mut());
            (self.api.destroy_context)(self.display, self.context);
            (self.api.terminate)(self.display);
            (self.api.gbm_device_destroy)(self.gbm);
        }
    }
}
