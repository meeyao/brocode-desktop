//! Native Ghostty terminal core (`libghostty-vt` `v1.4.0`) and off-main-thread PTY session
//! worker for Brocode.

// C-FFI wrapper for `libghostty-vt.a`.
#![allow(unsafe_code)]

mod connection;
pub mod core;
pub mod ffi;
mod ffi_fns;
mod find;
mod grapheme;
mod inbox;
mod input;
mod marks;
pub mod persistent_pty;
mod publish;
pub mod pty;
mod render_extract;
mod selection;
pub mod theme;
pub mod types;

pub use core::VtCore;
pub use ffi::GhosttyKey;
pub use grapheme::Grapheme;
pub use marks::CommandMark;
pub use pty::{
    MAX_PERSIST_SCROLLBACK, PersistedTerminal, RestoreState, SessionPersistHandle, SessionStatus,
    StreamHandle, TerminalConfig, TerminalSession,
};
pub use theme::{Rgb, TerminalTheme};
pub use types::{
    BgSpan, CellFlags, CursorShape, CursorSnapshot, FindHit, FindView, GridPoint, KeyAction, KeyInput, MouseBtn,
    MouseInput, MouseKind, RenderedCell, ScrollOp, ScrollbarSnapshot, SelectionOp, TerminalRow,
    TerminalSnapshot, UnderlineKind, VtEvent,
};

