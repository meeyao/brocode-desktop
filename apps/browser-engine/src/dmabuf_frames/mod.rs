//! Linux GPU frames: each CEF dmabuf is copied, on the GPU with EGL, into one of a tab's few
//! linear dmabufs, which the app imports into its Vulkan device once per buffer
//! (`rune::native_surface::linux`) after the engine passes their file descriptors over the
//! socket (`SCM_RIGHTS`).
//!
//! As on Windows (`d3d_frames.rs`), CEF's buffer is only valid during `OnAcceleratedPaint`, so
//! the copy finishes (an EGL fence the engine waits on) before the callback returns, and so
//! before the app hears of the frame. CEF 156 hands the buffer over once the GPU work that fills
//! it is submitted, not done (viz's `populates_mappable_shared_image` is false for Linux's native
//! handles), so the copy first waits for the buffer's writers (`acquire`). The app says which
//! frame of which set of buffers it has read (`HostMsg::FrameRead`) once its GPU has finished
//! copying it; a buffer is written again only after. With every buffer unread the frame is
//! dropped, and the read that frees one repaints the tab.
//!
//! At startup `Gpu::new` probes allocating and attaching a single linear `ARGB8888` GBM buffer
//! so drivers that cannot produce linear renderable dmabufs (such as NVIDIA's proprietary GBM
//! driver) start directly in `Transport::Bgra` without opening shared-texture tabs.
//!
//! When the app rejects imported dmabufs (`HostMsg::DmabufFailed`), frames are read back from
//! CEF's buffer to BGRA paints (`State::Readback`), keeping live tabs open. If a producer
//! allocation, copy, or readback fails (or readback produces an all-zero transparent buffer),
//! tabs reopen without shared textures (`Transport::Bgra`) so CEF paints BGRA directly.

mod egl;

use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use cef::sys::cef_color_type_t;
use cef::{AcceleratedPaintInfo, BrowserHost, ImplBrowserHost, PaintElementType};

use crate::output::{Output, Paint};
use crate::wire::EngineMsg;

const SLOTS: usize = 3;
/// Small linear render-target size probed in `Gpu::new` before committing to dmabuf transport.
const PROBE_SIZE: u32 = 16;
const ARGB8888: u32 = u32::from_le_bytes(*b"AR24");
const ABGR8888: u32 = u32::from_le_bytes(*b"AB24");
const MOD_INVALID: u64 = 0x00ff_ffff_ffff_ffff;

/// Ids of the engine's sets of buffers, unique for its life, so a read meant for an old set
/// frees nothing in a new one.
static POOLS: AtomicU64 = AtomicU64::new(1);

/// The engine's EGL context on the app's GPU. GL calls on it happen on CEF's UI thread only.
pub struct Gpu {
    egl: egl::Egl,
    reads_bgra: bool,
    pub node: String,
}

impl Gpu {
    /// A context on render node `node` (`/dev/dri/renderD128`), verified to allocate and render
    /// into a linear single-plane `ARGB8888` dmabuf, then released from this thread.
    pub fn new(node: &str) -> Result<Arc<Self>, String> {
        let egl = egl::Egl::open(node)?;
        if !egl.make_current() {
            return Err(format!("couldn't use the EGL context on {node}"));
        }
        let reads_bgra = egl.reads_bgra();
        let probe = Slot::new(&egl, PROBE_SIZE, PROBE_SIZE);
        match probe {
            Ok((slot, _)) => {
                slot.destroy(&egl, true);
                egl.release();
                Ok(Arc::new(Self { egl, reads_bgra, node: node.to_owned() }))
            }
            Err(reason) => {
                egl.release();
                Err(reason)
            }
        }
    }
}

/// CEF's frame as an EGL image, for this paint only.
struct Source<'a> {
    gpu: &'a Gpu,
    image: egl::Image,
    size: (u32, u32),
}

impl<'a> Source<'a> {
    fn import(gpu: &'a Gpu, info: &AcceleratedPaintInfo) -> Result<Self, String> {
        let size = (info.extra.coded_size.width as u32, info.extra.coded_size.height as u32);
        if info.plane_count != 1 || size.0 == 0 || size.1 == 0 {
            return Err(format!("CEF gave {} planes at {}x{}", info.plane_count, size.0, size.1));
        }
        let fourcc = match cef_color_type_t::from(info.format) {
            cef_color_type_t::CEF_COLOR_TYPE_BGRA_8888 => ARGB8888,
            cef_color_type_t::CEF_COLOR_TYPE_RGBA_8888 => ABGR8888,
            _ => return Err("CEF gave an unknown pixel format".into()),
        };
        if !gpu.egl.make_current() {
            return Err("couldn't use the EGL context".into());
        }
        let p = &info.planes[0];
        acquire(p.fd);
        let modifier = (info.modifier != MOD_INVALID).then_some(info.modifier);
        let plane = egl::Plane { fd: p.fd, stride: p.stride, offset: p.offset as u32, modifier };
        let image = gpu.egl.import(plane, size.0, size.1, fourcc);
        if image.is_null() {
            return Err("EGL couldn't import Chromium's frame".into());
        }
        Ok(Self { gpu, image, size })
    }

    /// Runs `f` with the image bound as the read framebuffer. False when it can't be.
    fn bound(&self, f: impl FnOnce(&egl::Api)) -> bool {
        let api = &self.gpu.egl.api;
        let (mut texture, mut framebuffer) = (0, 0);
        // SAFETY: GL calls on the current context; the texture and framebuffer live for this call.
        unsafe {
            while (api.get_error)() != egl::NO_ERROR {}
            (api.gen_textures)(1, &mut texture);
            (api.bind_texture)(egl::TEXTURE_2D, texture);
            (api.image_target_texture)(egl::TEXTURE_2D, self.image);
            (api.gen_framebuffers)(1, &mut framebuffer);
            (api.bind_framebuffer)(egl::READ_FRAMEBUFFER, framebuffer);
            (api.framebuffer_texture_2d)(egl::READ_FRAMEBUFFER, egl::COLOR_ATTACHMENT0, egl::TEXTURE_2D, texture, 0);
            let complete = (api.get_error)() == egl::NO_ERROR
                && (api.check_framebuffer_status)(egl::READ_FRAMEBUFFER) == egl::FRAMEBUFFER_COMPLETE;
            if complete {
                f(api);
            }
            (api.bind_framebuffer)(egl::READ_FRAMEBUFFER, 0);
            (api.bind_texture)(egl::TEXTURE_2D, 0);
            (api.delete_framebuffers)(1, &framebuffer);
            (api.delete_textures)(1, &texture);
            complete
        }
    }

    /// The frame's pixels, top row first, BGRA.
    fn read_back(&self, frame_id: u64) -> Result<Paint, String> {
        let (w, h) = self.size;
        let mut bgra = vec![0u8; w as usize * h as usize * 4];
        let mut bgr = self.gpu.reads_bgra;
        let mut gl_err = egl::NO_ERROR;
        let read = self.bound(|api| {
            // SAFETY: `bgra` holds w × h tightly packed 4-byte pixels (GL's default pack alignment is 4).
            unsafe {
                while (api.get_error)() != egl::NO_ERROR {}
                let format = if bgr { egl::BGRA_EXT } else { egl::RGBA };
                (api.read_pixels)(0, 0, w as i32, h as i32, format, egl::UNSIGNED_BYTE, bgra.as_mut_ptr().cast());
                gl_err = (api.get_error)();
                // GLES 3 always accepts `RGBA/UNSIGNED_BYTE`, whereas `BGRA_EXT` can still fail
                // with `GL_INVALID_OPERATION` on an EGLImage attachment even when the extension is
                // advertised. Retry with `RGBA` before giving up on readback.
                if bgr && gl_err != egl::NO_ERROR {
                    bgr = false;
                    while (api.get_error)() != egl::NO_ERROR {}
                    (api.read_pixels)(0, 0, w as i32, h as i32, egl::RGBA, egl::UNSIGNED_BYTE, bgra.as_mut_ptr().cast());
                    gl_err = (api.get_error)();
                }
            }
        });
        if !read || gl_err != egl::NO_ERROR {
            return Err(if !read {
                "GL couldn't read Chromium's frame".into()
            } else {
                format!("GL readPixels failed ({gl_err:#x})")
            });
        }
        // Every engine browser paints over an opaque `BASE_COLOR`, so an all-zero buffer (every
        // pixel transparent) means readback did not receive Chromium's pixels (e.g. NVIDIA EGL
        // external-image readback). Reject it so the engine reopens tabs in direct BGRA mode.
        if bgra.iter().all(|&b| b == 0) {
            return Err("GL readback returned an empty frame".into());
        }
        if !bgr {
            for pixel in bgra.as_chunks_mut::<4>().0 {
                pixel.swap(0, 2);
            }
        }
        Ok(Paint { frame_id, width: w, height: h, bgra })
    }
}

impl Drop for Source<'_> {
    fn drop(&mut self) {
        // SAFETY: the image was made in `import` and is destroyed once.
        unsafe { (self.gpu.egl.api.destroy_image)(self.gpu.egl.display, self.image) };
    }
}

/// Waits (up to a second) for the GPU work still writing dmabuf `fd`, as far as the kernel
/// tracks it: its implicit write fences, exported as a sync file (`DMA_BUF_IOCTL_EXPORT_SYNC_FILE`,
/// Linux 6.0).
#[allow(unsafe_code)]
fn acquire(fd: i32) {
    #[repr(C)]
    struct ExportSyncFile {
        flags: u32,
        fd: i32,
    }
    const DMA_BUF_IOCTL_EXPORT_SYNC_FILE: libc::c_ulong = 0xC008_6202;
    const DMA_BUF_SYNC_READ: u32 = 1;
    let mut arg = ExportSyncFile { flags: DMA_BUF_SYNC_READ, fd: -1 };
    // SAFETY: the ioctl fills `arg` with a new sync file descriptor this then owns.
    if unsafe { libc::ioctl(fd, DMA_BUF_IOCTL_EXPORT_SYNC_FILE as _, &mut arg) } != 0 || arg.fd < 0 {
        return;
    }
    // SAFETY: as above.
    let sync = unsafe { OwnedFd::from_raw_fd(arg.fd) };
    let mut poll = libc::pollfd { fd: sync.as_raw_fd(), events: libc::POLLIN, revents: 0 };
    // SAFETY: one pollfd over a live descriptor.
    unsafe { libc::poll(&mut poll, 1, 1000) };
}

/// A set of buffers the app imports before the frames in them: one fd per buffer, in order.
pub struct Pool {
    pub id: u64,
    pub width: u32,
    pub height: u32,
    pub stride: u32,
    pub offset: u32,
    pub modifier: u64,
    pub fds: Vec<OwnedFd>,
}

/// A tab's buffers on the engine's GPU.
pub struct Buffers {
    gpu: Arc<Gpu>,
    pool: u64,
    size: (u32, u32),
    layout: (u32, u32, u64),
    slots: Vec<Slot>,
}

/// What a buffer holds.
#[derive(Clone, Copy, PartialEq)]
enum Holds {
    Free,
    /// A frame the app hasn't read yet.
    Unread(u64),
}

struct Slot {
    bo: egl::Bo,
    fd: OwnedFd,
    image: egl::Image,
    texture: u32,
    framebuffer: u32,
    holds: Holds,
}

impl Slot {
    /// Allocates one linear `ARGB8888` GBM buffer, exports its dmabuf fd, imports it as an EGL
    /// image, and attaches it to a GLES draw framebuffer (context must be current).
    fn new(egl: &egl::Egl, width: u32, height: u32) -> Result<(Self, (u32, u32, u64)), String> {
        let api = &egl.api;
        let bo = egl.linear_bo(width, height, ARGB8888);
        if bo.is_null() {
            return Err("GBM couldn't make a linear buffer".into());
        }
        // SAFETY: queries and fd export on the live buffer; wrap valid fd immediately so early
        // returns never leak it.
        let (raw_fd, stride, offset, modifier, planes) = unsafe {
            (
                (api.gbm_bo_get_fd)(bo),
                (api.gbm_bo_get_stride)(bo),
                (api.gbm_bo_get_offset)(bo, 0),
                (api.gbm_bo_get_modifier)(bo),
                (api.gbm_bo_get_plane_count)(bo),
            )
        };
        let fd = (raw_fd >= 0).then(|| {
            // SAFETY: `gbm_bo_get_fd` returns a new fd the caller owns.
            unsafe { OwnedFd::from_raw_fd(raw_fd) }
        });
        let Some(fd) = fd.filter(|_| planes == 1) else {
            // SAFETY: the buffer was made above and nothing else holds it.
            unsafe { (api.gbm_bo_destroy)(bo) };
            return Err("GBM's linear buffer isn't one exportable plane".into());
        };
        let image = egl.import(
            egl::Plane { fd: fd.as_raw_fd(), stride, offset, modifier: Some(modifier) },
            width,
            height,
            ARGB8888,
        );
        let (mut texture, mut framebuffer) = (0, 0);
        // SAFETY: GL calls on the current context over the image made above.
        let complete = !image.is_null()
            && unsafe {
                while (api.get_error)() != egl::NO_ERROR {}
                (api.gen_textures)(1, &mut texture);
                (api.bind_texture)(egl::TEXTURE_2D, texture);
                (api.image_target_texture)(egl::TEXTURE_2D, image);
                (api.gen_framebuffers)(1, &mut framebuffer);
                (api.bind_framebuffer)(egl::DRAW_FRAMEBUFFER, framebuffer);
                (api.framebuffer_texture_2d)(egl::DRAW_FRAMEBUFFER, egl::COLOR_ATTACHMENT0, egl::TEXTURE_2D, texture, 0);
                let ok = (api.get_error)() == egl::NO_ERROR
                    && (api.check_framebuffer_status)(egl::DRAW_FRAMEBUFFER) == egl::FRAMEBUFFER_COMPLETE;
                (api.bind_framebuffer)(egl::DRAW_FRAMEBUFFER, 0);
                (api.bind_texture)(egl::TEXTURE_2D, 0);
                ok
            };
        let slot = Slot { bo, fd, image, texture, framebuffer, holds: Holds::Free };
        if !complete {
            slot.destroy(egl, true);
            return Err("EGL can't render to a linear buffer".into());
        }
        Ok((slot, (stride, offset, modifier)))
    }

    fn destroy(self, egl: &egl::Egl, current: bool) {
        // SAFETY: the slot's GL objects, image and buffer were made in `new` on this context,
        // and are freed once here. `self.fd` closes on drop.
        unsafe {
            if current {
                if self.framebuffer != 0 {
                    (egl.api.delete_framebuffers)(1, &self.framebuffer);
                }
                if self.texture != 0 {
                    (egl.api.delete_textures)(1, &self.texture);
                }
            }
            if !self.image.is_null() {
                (egl.api.destroy_image)(egl.display, self.image);
            }
            (egl.api.gbm_bo_destroy)(self.bo);
        }
    }
}

// SAFETY: only CEF's UI thread touches these, behind the tab's mutex.
unsafe impl Send for Buffers {}

impl Buffers {
    fn new(gpu: &Arc<Gpu>, (width, height): (u32, u32)) -> Result<Self, String> {
        let egl = &gpu.egl;
        let mut buffers = Buffers {
            gpu: gpu.clone(),
            pool: POOLS.fetch_add(1, Ordering::Relaxed),
            size: (width, height),
            layout: (0, 0, 0),
            slots: Vec::with_capacity(SLOTS),
        };
        for i in 0..SLOTS {
            let (slot, layout) = Slot::new(egl, width, height)?;
            if i == 0 {
                buffers.layout = layout;
            }
            buffers.slots.push(slot);
        }
        Ok(buffers)
    }

    /// The set as the app receives it, with copies of the buffers' descriptors.
    fn pool(&self) -> Result<Pool, String> {
        let fds = self.slots.iter().map(|s| s.fd.try_clone()).collect::<Result<Vec<_>, _>>().map_err(|e| format!("dup dmabuf: {e}"))?;
        let (stride, offset, modifier) = self.layout;
        Ok(Pool { id: self.pool, width: self.size.0, height: self.size.1, stride, offset, modifier, fds })
    }

    /// Copies `source` into buffer `slot`, finished on the GPU when this returns.
    fn copy(&self, source: &Source, slot: usize) -> Result<(), String> {
        let (w, h) = (self.size.0 as i32, self.size.1 as i32);
        let target = self.slots[slot].framebuffer;
        let mut gl_err = egl::NO_ERROR;
        let bound = source.bound(|api| {
            // SAFETY: GL calls on the current context; the slot's framebuffer lives as long as it.
            unsafe {
                (api.bind_framebuffer)(egl::DRAW_FRAMEBUFFER, target);
                (api.blit_framebuffer)(0, 0, w, h, 0, 0, w, h, egl::COLOR_BUFFER_BIT, egl::NEAREST);
                gl_err = (api.get_error)();
                (api.bind_framebuffer)(egl::DRAW_FRAMEBUFFER, 0);
            }
        });
        if !bound || gl_err != egl::NO_ERROR {
            return Err("GL couldn't read Chromium's frame".into());
        }
        if !self.gpu.egl.finish() {
            return Err("the GPU couldn't copy Chromium's frame".into());
        }
        Ok(())
    }

    fn free_slot(&self) -> Option<usize> {
        self.slots.iter().position(|s| s.holds == Holds::Free)
    }
}

impl Drop for Buffers {
    fn drop(&mut self) {
        let egl = &self.gpu.egl;
        let current = egl.make_current();
        for slot in self.slots.drain(..) {
            slot.destroy(egl, current);
        }
    }
}

/// How a tab's frames reach the app.
pub enum State {
    /// No buffers: the next paint makes them (the first paint, and after a resize, a new app
    /// connection or the tab being hidden).
    Empty,
    /// Buffers the app has; `owed` when a frame was dropped with every buffer unread.
    Shared { buffers: Buffers, owed: Owed },
    /// The app can't use the buffers: frames are read back to BGRA paints.
    Readback,
    /// Producer allocation, copy, or readback failed; waiting for tabs to reopen without shared
    /// textures.
    Failed,
}

#[derive(Clone, Copy, PartialEq)]
pub enum Owed {
    Nothing,
    Repaint,
}

pub enum Event {
    /// New buffers for the tab's size, and their pool for the app.
    Made(Buffers, Pool),
    Copied { slot: usize, frame_id: u64 },
    /// A frame came with every buffer unread.
    Full,
    /// The app has read `frame_id`, and every earlier frame, of pool `pool`.
    Read { pool: u64, frame_id: u64 },
    /// The tab was hidden, or a new app connected: the buffers go.
    Released,
    /// The app rejected imported dmabufs: read frames back from now on.
    ReadBack,
    /// The engine couldn't allocate, copy, or read frames: reopen tabs without shared textures.
    ProducerFailed(String),
}

pub enum Effect {
    SendPool(Pool),
    SendFrame { pool: u64, slot: usize, frame_id: u64 },
    SendBgra(Paint),
    Repaint,
    Failed(Failure),
}

/// Why frames can't come as dmabufs.
#[derive(Debug)]
pub enum Failure {
    /// The app couldn't import the engine's dmabufs: try reading frames back first.
    Share(String),
    /// The engine couldn't allocate, copy, or read frames: reopen tabs without shared textures.
    Read(String),
}

pub fn step(state: &mut State, event: Event) -> Vec<Effect> {
    match event {
        Event::Made(buffers, pool) => {
            *state = State::Shared { buffers, owed: Owed::Nothing };
            vec![Effect::SendPool(pool)]
        }
        Event::Copied { slot, frame_id } => match state {
            State::Shared { buffers, .. } => {
                buffers.slots[slot].holds = Holds::Unread(frame_id);
                vec![Effect::SendFrame { pool: buffers.pool, slot, frame_id }]
            }
            State::Empty | State::Readback | State::Failed => Vec::new(),
        },
        Event::Full => {
            if let State::Shared { owed, .. } = state {
                *owed = Owed::Repaint;
            }
            Vec::new()
        }
        Event::Read { pool, frame_id } => match state {
            State::Shared { buffers, owed } if buffers.pool == pool => {
                // The app shows only the newest frame, so a read of a frame frees every earlier one.
                for slot in &mut buffers.slots {
                    if matches!(slot.holds, Holds::Unread(f) if f <= frame_id) {
                        slot.holds = Holds::Free;
                    }
                }
                if *owed == Owed::Repaint && buffers.free_slot().is_some() {
                    *owed = Owed::Nothing;
                    vec![Effect::Repaint]
                } else {
                    Vec::new()
                }
            }
            _ => Vec::new(),
        },
        Event::Released => {
            if let State::Shared { .. } = state {
                *state = State::Empty;
            }
            Vec::new()
        }
        Event::ReadBack => match state {
            State::Empty | State::Shared { .. } => {
                *state = State::Readback;
                vec![Effect::Repaint]
            }
            State::Readback | State::Failed => Vec::new(),
        },
        Event::ProducerFailed(reason) => match state {
            State::Failed => Vec::new(),
            State::Empty | State::Shared { .. } | State::Readback => {
                *state = State::Failed;
                vec![Effect::Failed(Failure::Read(reason))]
            }
        },
    }
}

/// One tab's frames.
pub struct TabFrames {
    gpu: Arc<Gpu>,
    state: State,
}

impl TabFrames {
    pub fn new(gpu: Arc<Gpu>, read_back: bool) -> Self {
        Self { gpu, state: if read_back { State::Readback } else { State::Empty } }
    }

    pub fn step(&mut self, event: Event) -> Vec<Effect> {
        step(&mut self.state, event)
    }

    /// CEF painted frame `frame_id` (its buffer valid during this call only).
    pub fn paint(&mut self, info: &AcceleratedPaintInfo, frame_id: u64) -> Vec<Effect> {
        if matches!(self.state, State::Failed) {
            return Vec::new();
        }
        let gpu = self.gpu.clone();
        let source = match Source::import(&gpu, info) {
            Ok(source) => source,
            Err(e) => return self.step(Event::ProducerFailed(e)),
        };
        let mut effects = Vec::new();
        match &self.state {
            State::Readback => {
                return match source.read_back(frame_id) {
                    Ok(paint) => vec![Effect::SendBgra(paint)],
                    Err(e) => self.step(Event::ProducerFailed(e)),
                };
            }
            State::Failed => return Vec::new(),
            State::Shared { buffers, .. } if buffers.size == source.size => {}
            State::Shared { .. } | State::Empty => {
                // The old buffers go first, so a resize never holds two sets.
                self.state = State::Empty;
                match Buffers::new(&gpu, source.size).and_then(|b| b.pool().map(|p| (b, p))) {
                    Ok((buffers, pool)) => effects.extend(self.step(Event::Made(buffers, pool))),
                    Err(e) => return self.step(Event::ProducerFailed(e)),
                }
            }
        }
        let State::Shared { buffers, .. } = &self.state else { return effects };
        let Some(slot) = buffers.free_slot() else {
            effects.extend(self.step(Event::Full));
            return effects;
        };
        match buffers.copy(&source, slot) {
            Ok(()) => effects.extend(self.step(Event::Copied { slot, frame_id })),
            Err(e) => return self.step(Event::ProducerFailed(e)),
        }
        effects
    }
}

/// Runs tab `tab_id`'s effects: messages to the app, repaints, and failures for the engine.
pub fn run(effects: Vec<Effect>, tab_id: u64, out: &Output, host: Option<&BrowserHost>, failed: &dyn Fn(Failure)) {
    for effect in effects {
        match effect {
            Effect::SendPool(p) => {
                let msg = EngineMsg::DmabufPool {
                    tab_id,
                    pool: p.id,
                    buffers: p.fds.len() as u32,
                    width: p.width,
                    height: p.height,
                    stride: p.stride,
                    offset: p.offset,
                    modifier: p.modifier,
                };
                let _ = out.send_with(msg, p.fds);
            }
            Effect::SendFrame { pool, slot, frame_id } => {
                let _ = out.send(EngineMsg::DmabufFrame { tab_id, frame_id, pool, slot: slot as u32 });
            }
            Effect::SendBgra(paint) => out.paint(tab_id, paint),
            Effect::Repaint => {
                if let Some(host) = host {
                    host.invalidate(PaintElementType::default());
                }
            }
            Effect::Failed(failure) => failed(failure),
        }
    }
}

/// How the engine's frames reach the app. It starts with dmabufs when the app asked for them
/// and the engine's EGL context came up. When the app can't use them, frames are read back to
/// BGRA paints; when Chromium's own frames can't be read, tabs are opened again without shared
/// textures, painting BGRA on the CPU path. Both last for the engine's life.
pub enum Transport {
    Dmabuf(Arc<Gpu>),
    Readback(Arc<Gpu>),
    Bgra,
}

pub enum TransportEffect {
    /// Every tab reads its frames back from now on.
    ReadBackTabs,
    /// Open every tab again without shared textures.
    ReopenTabs,
}

impl Transport {
    pub fn step(&mut self, failure: Failure) -> Option<TransportEffect> {
        let (next, effect) = match (&*self, failure) {
            (Transport::Dmabuf(gpu), Failure::Share(reason)) => {
                eprintln!("browser engine: transport=readback ({reason})");
                (Transport::Readback(gpu.clone()), TransportEffect::ReadBackTabs)
            }
            (Transport::Dmabuf(_) | Transport::Readback(_), Failure::Read(reason)) => {
                eprintln!("browser engine: transport=bgra ({reason})");
                (Transport::Bgra, TransportEffect::ReopenTabs)
            }
            (Transport::Readback(_), Failure::Share(_)) | (Transport::Bgra, _) => return None,
        };
        *self = next;
        Some(effect)
    }

    /// A new tab's frames: None when it paints without shared textures.
    pub fn frames(&self) -> Option<TabFrames> {
        match self {
            Transport::Dmabuf(gpu) => Some(TabFrames::new(gpu.clone(), false)),
            Transport::Readback(gpu) => Some(TabFrames::new(gpu.clone(), true)),
            Transport::Bgra => None,
        }
    }
}
