//! Parts agents compose into notes and chat replies: one catalog, one outline grammar, one checker.
//!
//! A ```` ```ui ```` fence holds an outline, a part name first on every line and kids indented two
//! spaces under it. Compact cards and Brocode sessions draw the same outline with their own
//! theme ([`view`], behind the `view` feature), and the checker gives agents errors they can act on.
//!
//! ```text
//! row
//!   box Today
//!     item {risk} Voice reply · pasted into the bubble
//!   arrow
//!   box After
//!     say Alex {voice}: filter Lumen to Hex
//! ```
//!
//! Without `view` the crate is plain Rust, so CLIs, Workers and Wasm link it too.

mod catalog;
mod check;
mod html;
mod note;
mod outline;
#[cfg(feature = "view")]
pub mod view;

pub use catalog::{CATALOG, Kids, PartSpec, Tone, docs, spec};
pub use check::{BudgetProfile, Issue, check, check_outline};
pub use html::to_html;
pub use note::{Block, parse_note};
pub use outline::{Node, Part, parse_outline, to_plain};

