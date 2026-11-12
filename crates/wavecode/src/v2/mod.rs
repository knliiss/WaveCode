//! Experimental WaveCode V2: W-anchored, variable-length optical frames.
//!
//! This module is intentionally separate from V1. Existing V1 scan, render,
//! symbol ordering, error correction and printed marks are not changed.
pub mod anchor;
mod codec;

pub use codec::{decode, encode, encode_with_layout, Content, Decoded, Frame, Layout};
