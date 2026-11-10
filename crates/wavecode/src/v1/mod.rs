mod codec;
#[cfg(feature = "raster")]
mod raster;
mod rs;
mod visual;

#[cfg(feature = "raster")]
pub use raster::{render_png, scan_gray, scan_image};

pub use codec::{
    decode, decode_observations, decode_soft, decode_with_erasures, encode, Observation,
    SoftObservation, SymbolHypothesis, CORRECTION_SYMBOLS, DATA_COUNT, FORMAT_VERSION,
    SYMBOL_COUNT,
};
pub use visual::{
    decode_frame, decode_svg, frame_from_symbols, render_svg, render_svg_with_profile, Bar,
    RenderProfile, FRAME_BARS, LEVELS_PER_RAIL,
};
