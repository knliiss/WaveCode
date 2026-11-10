//! Browser- and Node-compatible WebAssembly API backed by the same V1 core.
use serde_json::json;
use wasm_bindgen::prelude::*;
use wavecode::{v1, MusicCode, Orientation};

fn js_error(error: impl std::fmt::Display) -> JsValue {
    JsValue::from_str(&error.to_string())
}

fn dimensions(width: u32, height: u32) -> Result<v1::RenderProfile, JsValue> {
    if width == 0 || height == 0 || u64::from(width) * u64::from(height) > 20_000_000 {
        return Err(js_error("invalid output canvas dimensions"));
    }
    Ok(v1::RenderProfile::new(f64::from(width), f64::from(height)))
}

#[wasm_bindgen]
pub fn abi_version() -> u32 { 1 }

#[wasm_bindgen]
pub fn format_version() -> u32 { u32::from(v1::FORMAT_VERSION) }

#[wasm_bindgen]
pub fn encode_url(url: &str, moment_override: Option<u16>) -> Result<Vec<u8>, JsValue> {
    if url.len() > 4096 {
        return Err(js_error("track URL exceeds maximum length"));
    }
    let mut code = MusicCode::from_url(url).map_err(js_error)?;
    if let Some(moment) = moment_override {
        code.moment_seconds = Some(moment);
    }
    v1::encode(&code).map_err(js_error)
}

#[wasm_bindgen]
pub fn decode_symbols(symbols: &[u8]) -> Result<String, JsValue> {
    v1::decode(symbols).map_err(js_error)?.code.to_url().map_err(js_error)
}

#[wasm_bindgen]
pub fn render_svg(symbols: &[u8], width: u32, height: u32) -> Result<String, JsValue> {
    v1::render_svg_with_profile(symbols, dimensions(width, height)?).map_err(js_error)
}

#[wasm_bindgen]
pub fn render_png(symbols: &[u8], width: u32, height: u32) -> Result<Vec<u8>, JsValue> {
    v1::render_png(symbols, dimensions(width, height)?).map_err(js_error)
}

#[wasm_bindgen]
pub fn scan_image(image: &[u8]) -> Result<String, JsValue> {
    if image.len() > 16 * 1024 * 1024 {
        return Err(js_error("image is too large"));
    }
    v1::scan_image(image).map_err(js_error)?.decoded.code.to_url().map_err(js_error)
}

#[wasm_bindgen]
pub fn scan_all(image: &[u8]) -> Result<String, JsValue> {
    if image.len() > 16 * 1024 * 1024 {
        return Err(js_error("image is too large"));
    }
    let found = v1::scan_image_all(image).map_err(js_error)?;
    let mut codes = Vec::with_capacity(found.len());
    for candidate in found {
        codes.push(json!({
            "url": candidate.result.decoded.code.to_url().map_err(js_error)?,
            "orientation": match candidate.result.orientation {
                Orientation::Upright => "upright",
                Orientation::UpsideDown => "upside_down",
            },
            "corrected_symbols": candidate.result.decoded.corrected_symbols,
            "bounds": {
                "left": candidate.bounds.left,
                "top": candidate.bounds.top,
                "width": candidate.bounds.width,
                "height": candidate.bounds.height,
            }
        }));
    }
    serde_json::to_string(&codes).map_err(js_error)
}
