//! Versioned JSON Lines messages shared by the reader and listening program.
//!
//! This crate intentionally has no model, playback or process dependencies.

mod codec;
mod message;

pub use codec::*;
pub use message::*;

/// Supported protocol major version.
pub const PROTOCOL_VERSION: u32 = 1;
/// Maximum encoded message size, excluding the line terminator.
pub const MAX_MESSAGE_BYTES: usize = 16 * 1024 * 1024;

/// SHA-256 of the exact UTF-8 snapshot, before any speech normalization.
pub fn text_hash(text: &str) -> String {
    use sha2::{Digest, Sha256};
    format!("{:x}", Sha256::digest(text.as_bytes()))
}
