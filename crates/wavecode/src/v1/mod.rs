mod codec;
mod rs;
mod visual;

pub use codec::{
    decode, decode_with_erasures, encode, CORRECTION_SYMBOLS, DATA_COUNT, FORMAT_VERSION,
    SYMBOL_COUNT,
};
pub use visual::{
    decode_frame, decode_svg, frame_from_symbols, render_svg, Bar, FRAME_BARS, LEVELS_PER_RAIL,
};
