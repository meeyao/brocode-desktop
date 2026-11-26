//! Timeline and terminal quoting, composer comment cards, floating quote-comment editor, and
//! inline code/diff line comments for Brocode.
//!
//! Both [`quote`] and [`line_comments`] share the compact comment field machine
//! (`quote::comment::Field`), `@` path search, and the composer's pending comment cards inside
//! one cohesive feature crate with object-safe host boundaries ([`quote::QuoteHost`] and
//! [`line_comments::LineCommentsHost`]).

#![forbid(unsafe_code)]

pub mod line_comments;
pub mod quote;

pub use line_comments::{LineComments, LineCommentsHost, Range, Spot, Surface};
pub use quote::{FollowUp, OUTLINE_QUOTE, Pending, QuoteHost, Quotes, SendResult};
