//! Experimental WaveCode V2: W-anchored, variable-length optical frames.
//!
//! This module is intentionally separate from V1. Existing V1 scan, render,
//! symbol ordering, error correction and printed marks are not changed.
mod adaptive;
pub mod anchor;
mod codec;
#[cfg(feature = "raster")]
pub mod raster;
pub mod visual;

pub use codec::{
    decode, encode, encode_adaptive, encode_with_layout, Content, Decoded, Frame, Layout,
};
