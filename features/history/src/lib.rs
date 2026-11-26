//! Session timeline history and navigation for Brocode:
//! - [`message_nav`]: Previous/Next message stepping and the Jump to message dialog (`dialog.jump`).
//! - [`rewind`]: Rewind to message, `/undo`, `/redo`, `"Reverted"` toast undo, and message action menus.
//! - [`fork`]: Forking from a user message and the `"Fork from message"` dialog (`dialog.fork`).
//!
//! All crate entrypoints operate on non-generic `&dyn MessageNavHost` / `&mut dyn MessageNavHost`,
//! `&dyn RewindHost` / `&mut dyn RewindHost`, and `&dyn ForkHost` / `&mut dyn ForkHost` boundaries,
//! keeping thin `H: ... + 'static` wrappers strictly at `rune::El` / `Tx::element_at_in` callback
//! boundaries.

#![forbid(unsafe_code)]

pub mod fork;
pub mod message_nav;
pub mod rewind;

pub use fork::{FORK_OVERLAY_ID, ForkHost, ForkModal, ForkState, ForkableMessage};
pub use message_nav::{JUMP_OVERLAY_ID, MessageNav, MessageNavHost};
pub use rewind::{RewindHost, RewindState, SavedDraft, UndoRewind};
