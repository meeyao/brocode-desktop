//! Cohesive composer input subsystems for Brocode:
//! - [`attachments`]: inline/path file attachments, remote upload runner & state machine, upload
//!   progress toast, and reply-image fixture scene helper over [`session_view::images::MediaStore`].
//! - [`dropzone`]: file drag-and-drop overlay state machine and frosted/striped visual layer.
//! - [`prompt_history`]: persistent normal/shell prompt recall backed by host storage hooks.
//! - [`slash`]: `/` slash command catalog, fuzzy ranking, server command fetcher, execution, and popover.
//! - [`suggestions`]: unified composer suggestion/history state machine, keyboard routing, and
//!   history popover.
//!
//! All crate entrypoints operate on non-generic `&dyn *Host` / `&mut dyn *Host` boundaries,
//! keeping thin `H: ... + 'static` wrappers strictly on `rune::El` and `Tx::open_file` callbacks.

#![forbid(unsafe_code)]

pub mod attachments;
pub mod dropzone;
pub mod prompt_history;
pub mod slash;
pub mod suggestions;

pub use attachments::{Attach, AttachMut, AttachRef, AttachmentsHost, ContextFile, Copied, Failed};
pub use dropzone::{AttachmentInput, Dropzone, DropzoneHost, Due};
pub use prompt_history::{DocLookup, Entry, PROMPT_HISTORY_DOCUMENT, PromptHistory};
pub use slash::{BuiltinSlashCommand, Command as SlashCommand, Item as SlashItem, Slash, SlashHost};
pub use suggestions::{Suggest, SuggestionsHost};
