//! WaveCode V1 stable C ABI. Each owned output must be released with wavecode_free_buffer.
use serde_json::json;
use std::{
    panic::{catch_unwind, AssertUnwindSafe},
    ptr,
};
use wavecode::{v1, MusicCode, Orientation};

const MAX_URL: usize = 4096;
const MAX_IMAGE: usize = 16 * 1024 * 1024;

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct WaveCodeBuffer {
    pub ptr: *mut u8,
    pub len: usize,
}
impl Default for WaveCodeBuffer {
    fn default() -> Self {
        Self {
            ptr: ptr::null_mut(),
            len: 0,
        }
    }
}

#[repr(i32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WaveCodeStatus {
    Ok = 0,
    InvalidArgument = 1,
    EncodeError = 2,
    DecodeError = 3,
    ScanError = 4,
    InternalError = 255,
}

#[no_mangle]
pub extern "C" fn wavecode_abi_version() -> u32 {
    1
}

#[no_mangle]
pub extern "C" fn wavecode_format_version() -> u32 {
    u32::from(v1::FORMAT_VERSION)
}

#[no_mangle]
pub extern "C" fn wavecode_status_message(status: i32) -> *const std::ffi::c_char {
    let bytes: &'static [u8] = match status {
        0 => b"ok\0",
        1 => b"invalid argument\0",
        2 => b"unable to encode WaveCode\0",
        3 => b"unable to decode WaveCode\0",
        4 => b"unable to scan WaveCode image\0",
        255 => b"internal error\0",
        _ => b"unknown status\0",
    };
    bytes.as_ptr().cast()
}

fn dimensions(width: u32, height: u32) -> Result<v1::RenderProfile, WaveCodeStatus> {
    if width == 0 || height == 0 || u64::from(width) * u64::from(height) > 20_000_000 {
        return Err(WaveCodeStatus::InvalidArgument);
    }
    Ok(v1::RenderProfile::new(f64::from(width), f64::from(height)))
}

fn input<'a>(ptr: *const u8, len: usize, limit: usize) -> Result<&'a [u8], WaveCodeStatus> {
    if ptr.is_null() || len == 0 || len > limit || len > isize::MAX as usize {
        return Err(WaveCodeStatus::InvalidArgument);
    }
    // SAFETY: Callers must supply len initialized and readable bytes.
    Ok(unsafe { std::slice::from_raw_parts(ptr, len) })
}

fn encoded_symbols(ptr: *const u8, len: usize) -> Result<Vec<u8>, WaveCodeStatus> {
    let bytes = input(ptr, len, v1::SYMBOL_COUNT)?;
    if bytes.len() != v1::SYMBOL_COUNT || bytes.iter().any(|&symbol| symbol >= 64) {
        return Err(WaveCodeStatus::InvalidArgument);
    }
    Ok(bytes.to_vec())
}

fn allocate(bytes: Vec<u8>) -> WaveCodeBuffer {
    if bytes.is_empty() {
        return WaveCodeBuffer::default();
    }
    let len = bytes.len();
    let ptr = Box::into_raw(bytes.into_boxed_slice()).cast::<u8>();
    WaveCodeBuffer { ptr, len }
}

fn write_output<F>(target: *mut WaveCodeBuffer, operation: F) -> WaveCodeStatus
where
    F: FnOnce() -> Result<Vec<u8>, WaveCodeStatus>,
{
    if target.is_null() {
        return WaveCodeStatus::InvalidArgument;
    }
    // SAFETY: the pointer belongs to a writable WaveCodeBuffer for this call.
    unsafe { *target = WaveCodeBuffer::default() };
    match catch_unwind(AssertUnwindSafe(operation)) {
        Ok(Ok(bytes)) => {
            // SAFETY: the target pointer was validated above.
            unsafe { *target = allocate(bytes) };
            WaveCodeStatus::Ok
        }
        Ok(Err(error)) => error,
        Err(_) => WaveCodeStatus::InternalError,
    }
}

#[no_mangle]
pub extern "C" fn wavecode_v1_encode_url(
    url: *const u8,
    url_len: usize,
    moment_override: i32,
    out_symbols: *mut WaveCodeBuffer,
) -> WaveCodeStatus {
    write_output(out_symbols, || {
        if !(-1..=65535).contains(&moment_override) {
            return Err(WaveCodeStatus::InvalidArgument);
        }
        let url = std::str::from_utf8(input(url, url_len, MAX_URL)?)
            .map_err(|_| WaveCodeStatus::InvalidArgument)?;
        let mut code = MusicCode::from_url(url).map_err(|_| WaveCodeStatus::EncodeError)?;
        if moment_override >= 0 {
            code.moment_seconds = Some(moment_override as u16);
        }
        v1::encode(&code).map_err(|_| WaveCodeStatus::EncodeError)
    })
}

#[no_mangle]
pub extern "C" fn wavecode_v1_decode_symbols(
    ptr: *const u8,
    len: usize,
    out_url: *mut WaveCodeBuffer,
) -> WaveCodeStatus {
    write_output(out_url, || {
        let symbols = encoded_symbols(ptr, len)?;
        let decoded = v1::decode(&symbols).map_err(|_| WaveCodeStatus::DecodeError)?;
        decoded
            .code
            .to_url()
            .map(String::into_bytes)
            .map_err(|_| WaveCodeStatus::DecodeError)
    })
}

#[no_mangle]
pub extern "C" fn wavecode_v1_render_svg(
    ptr: *const u8,
    len: usize,
    width: u32,
    height: u32,
    out_svg: *mut WaveCodeBuffer,
) -> WaveCodeStatus {
    write_output(out_svg, || {
        let symbols = encoded_symbols(ptr, len)?;
        v1::render_svg_with_profile(&symbols, dimensions(width, height)?)
            .map(String::into_bytes)
            .map_err(|_| WaveCodeStatus::EncodeError)
    })
}

#[no_mangle]
pub extern "C" fn wavecode_v1_render_png(
    ptr: *const u8,
    len: usize,
    width: u32,
    height: u32,
    out_png: *mut WaveCodeBuffer,
) -> WaveCodeStatus {
    write_output(out_png, || {
        let symbols = encoded_symbols(ptr, len)?;
        v1::render_png(&symbols, dimensions(width, height)?)
            .map_err(|_| WaveCodeStatus::EncodeError)
    })
}

#[no_mangle]
pub extern "C" fn wavecode_v1_scan_image(
    image: *const u8,
    len: usize,
    out_url: *mut WaveCodeBuffer,
) -> WaveCodeStatus {
    write_output(out_url, || {
        let bytes = input(image, len, MAX_IMAGE)?;
        v1::scan_image(bytes)
            .map_err(|_| WaveCodeStatus::ScanError)?
            .decoded
            .code
            .to_url()
            .map(String::into_bytes)
            .map_err(|_| WaveCodeStatus::ScanError)
    })
}

#[no_mangle]
pub extern "C" fn wavecode_v1_scan_all(
    image: *const u8,
    len: usize,
    out_json: *mut WaveCodeBuffer,
) -> WaveCodeStatus {
    write_output(out_json, || {
        let bytes = input(image, len, MAX_IMAGE)?;
        let codes = v1::scan_image_all(bytes).map_err(|_| WaveCodeStatus::ScanError)?;
        let mut entries = Vec::with_capacity(codes.len());
        for found in codes {
            entries.push(json!({
                "url": found.result.decoded.code.to_url().map_err(|_| WaveCodeStatus::ScanError)?,
                "corrected_symbols": found.result.decoded.corrected_symbols,
                "orientation": match found.result.orientation {
                    Orientation::Upright => "upright",
                    Orientation::UpsideDown => "upside_down",
                },
                "bounds": {
                    "left": found.bounds.left,
                    "top": found.bounds.top,
                    "width": found.bounds.width,
                    "height": found.bounds.height,
                }
            }));
        }
        serde_json::to_vec(&entries).map_err(|_| WaveCodeStatus::InternalError)
    })
}

#[no_mangle]
pub extern "C" fn wavecode_free_buffer(buffer: WaveCodeBuffer) {
    if buffer.ptr.is_null() || buffer.len == 0 {
        return;
    }
    // SAFETY: ownership is exclusively transferred from an unmodified
    // WaveCode output; calling this twice or with foreign pointers is invalid.
    unsafe {
        drop(Box::from_raw(std::ptr::slice_from_raw_parts_mut(
            buffer.ptr, buffer.len,
        )));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn consume(buffer: WaveCodeBuffer) -> Vec<u8> {
        let bytes = unsafe { std::slice::from_raw_parts(buffer.ptr, buffer.len) }.to_vec();
        wavecode_free_buffer(buffer);
        bytes
    }

    #[test]
    fn ffi_roundtrip_and_scanning() {
        let url = b"https://music.youtube.com/watch?v=p-LBiCtzXu0&t=59";
        let mut out = WaveCodeBuffer::default();
        assert_eq!(wavecode_abi_version(), 1);
        assert_eq!(wavecode_format_version(), 1);
        assert_eq!(
            wavecode_v1_encode_url(url.as_ptr(), url.len(), -1, &mut out),
            WaveCodeStatus::Ok
        );
        let symbols = consume(out);
        assert_eq!(symbols.len(), 34);
        assert_eq!(
            wavecode_v1_decode_symbols(symbols.as_ptr(), symbols.len(), &mut out),
            WaveCodeStatus::Ok
        );
        assert_eq!(consume(out), url);
        assert_eq!(
            wavecode_v1_render_svg(symbols.as_ptr(), symbols.len(), 900, 240, &mut out),
            WaveCodeStatus::Ok
        );
        assert!(String::from_utf8(consume(out)).unwrap().contains("<svg"));
        assert_eq!(
            wavecode_v1_render_png(symbols.as_ptr(), symbols.len(), 900, 240, &mut out),
            WaveCodeStatus::Ok
        );
        let png = consume(out);
        assert!(png.starts_with(b"\x89PNG\r\n\x1a\n"));
        assert_eq!(
            wavecode_v1_scan_image(png.as_ptr(), png.len(), &mut out),
            WaveCodeStatus::Ok
        );
        assert_eq!(consume(out), url);
        assert_eq!(
            wavecode_v1_scan_all(png.as_ptr(), png.len(), &mut out),
            WaveCodeStatus::Ok
        );
        let entries: serde_json::Value = serde_json::from_slice(&consume(out)).unwrap();
        assert_eq!(entries[0]["url"], String::from_utf8_lossy(url).as_ref());
        assert_eq!(entries[0]["orientation"], "upright");
    }

    #[test]
    fn invalid_inputs_return_errors_and_no_allocation() {
        let mut out = WaveCodeBuffer::default();
        let invalid = b"invalid";
        assert_eq!(
            wavecode_v1_encode_url(ptr::null(), 4, -1, &mut out),
            WaveCodeStatus::InvalidArgument
        );
        assert!(out.ptr.is_null());
        assert_eq!(
            wavecode_v1_encode_url(invalid.as_ptr(), invalid.len(), -2, &mut out),
            WaveCodeStatus::InvalidArgument
        );
        let symbols = [255_u8; 34];
        assert_eq!(
            wavecode_v1_decode_symbols(symbols.as_ptr(), symbols.len(), &mut out),
            WaveCodeStatus::InvalidArgument
        );
        assert_eq!(
            wavecode_v1_render_png(symbols.as_ptr(), symbols.len(), 0, 0, &mut out),
            WaveCodeStatus::InvalidArgument
        );
        assert_eq!(
            wavecode_v1_encode_url(invalid.as_ptr(), invalid.len(), -1, ptr::null_mut()),
            WaveCodeStatus::InvalidArgument
        );
        wavecode_free_buffer(WaveCodeBuffer::default());
    }
}
