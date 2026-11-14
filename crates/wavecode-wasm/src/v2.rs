//! WaveCode V2 W-anchored browser and JavaScript interface.
//! The stable V1 ABI remains unchanged; every V2 entry point is explicitly prefixed.
use serde_json::{json, Value};
use wasm_bindgen::prelude::*;
use wavecode::{v2, MusicCode, Track};

use super::js_error;

const MAX_URL_BYTES: usize = 6096;
const MAX_IMAGE_BYTES: usize = 16 * 1024 * 1024;

fn layout_name(layout: v2::Layout) -> &'static str {
    match layout {
        v2::Layout::Compact => "compact",
        v2::Layout::Full => "full",
        v2::Layout::Dual => "dual",
    }
}

fn encoded_json(frame: &v2::Frame) -> Result<String, JsValue> {
    let decoded = v2::decode(frame).map_err(js_error)?;
    serde_json::to_string(&json!({
        "format": "v2-capsule-version-bch22",
        "version": v2::anchor::VERSION,
        "anchor": "capsule-version-bch22",
        "layout": layout_name(decoded.layout),
        "metadata": frame.metadata,
        "symbols": &frame.symbols,
    }))
    .map_err(js_error)
}

fn decoded_value(decoded: &v2::Decoded) -> Result<Value, wavecode::Error> {
    let urls = match &decoded.content {
        v2::Content::Single(code) => vec![code.to_url()?],
        v2::Content::DualLinks { first, second } => {
            vec![first.to_url()?, second.to_url()?]
        }
        v2::Content::Dual {
            spotify_id,
            youtube_music_id,
            moment_seconds,
        } => vec![
            MusicCode {
                track: Track::Spotify(spotify_id.clone()),
                moment_seconds: *moment_seconds,
            }
            .to_url()?,
            MusicCode {
                track: Track::YouTubeMusic(youtube_music_id.clone()),
                moment_seconds: *moment_seconds,
            }
            .to_url()?,
        ],
    };
    Ok(json!({
        "format": "v2-capsule-version-bch22",
        "version": v2::anchor::VERSION,
        "anchor": "capsule-version-bch22",
        "layout": layout_name(decoded.layout),
        "urls": urls,
        "corrected_symbols": decoded.corrected_symbols,
    }))
}

fn verified_frame(metadata: u8, symbols: &[u8]) -> Result<v2::Frame, JsValue> {
    let frame = v2::Frame {
        metadata,
        symbols: symbols.to_vec(),
    };
    v2::decode(&frame).map_err(js_error)?;
    Ok(frame)
}

fn profile(pitch: f64, height: f64, quiet: f64) -> v2::visual::RenderProfile {
    v2::visual::RenderProfile {
        pitch,
        height,
        quiet,
    }
}

fn visual_style(name: &str) -> Result<v2::visual::RenderStyle, JsValue> {
    match name {
        "classic" => Ok(v2::visual::RenderStyle::Classic),
        "compact" => Ok(v2::visual::RenderStyle::Compact),
        _ => Err(js_error(
            "unsupported V2 visual style; use classic or compact",
        )),
    }
}

/// Return a JSON frame. The routing header is transmitted as the FIRST
/// visible payload column, never as part of the version-only W.
#[wasm_bindgen]
pub fn v2_encode_url(url: &str, moment_override: Option<u16>) -> Result<String, JsValue> {
    if url.len() > MAX_URL_BYTES {
        return Err(js_error("track URL exceeds maximum length"));
    }
    let mut code = MusicCode::from_url(url).map_err(js_error)?;
    if let Some(moment) = moment_override {
        code.moment_seconds = Some(moment);
    }
    let frame = v2::encode_adaptive(&v2::Content::Single(code)).map_err(js_error)?;
    encoded_json(&frame)
}

/// Both independent identifiers must be known when the code is created.
#[wasm_bindgen]
pub fn v2_encode_dual(
    spotify_id: &str,
    youtube_music_id: &str,
    moment_seconds: Option<u16>,
) -> Result<String, JsValue> {
    let content = v2::Content::Dual {
        spotify_id: spotify_id.to_owned(),
        youtube_music_id: youtube_music_id.to_owned(),
        moment_seconds,
    };
    encoded_json(&v2::encode_adaptive(&content).map_err(js_error)?)
}

/// Encode two independently supported links, with optional per-link moments.
/// Existing Spotify + YouTube Music pairs sharing one moment retain the
/// original optimized V2 Dual profiles for backward compatibility.
#[wasm_bindgen]
pub fn v2_encode_links(
    first_url: &str,
    second_url: &str,
    first_moment_override: Option<u16>,
    second_moment_override: Option<u16>,
) -> Result<String, JsValue> {
    if first_url.len() > MAX_URL_BYTES || second_url.len() > MAX_URL_BYTES {
        return Err(js_error("track URL exceeds maximum length"));
    }
    let mut first = MusicCode::from_url(first_url).map_err(js_error)?;
    let mut second = MusicCode::from_url(second_url).map_err(js_error)?;
    if let Some(moment) = first_moment_override {
        first.moment_seconds = Some(moment);
    }
    if let Some(moment) = second_moment_override {
        second.moment_seconds = Some(moment);
    }
    first.to_url().map_err(js_error)?;
    second.to_url().map_err(js_error)?;
    let content = if let (Track::Spotify(spotify_id), Track::YouTubeMusic(youtube_music_id)) =
        (&first.track, &second.track)
    {
        if first.moment_seconds == second.moment_seconds {
            v2::Content::Dual {
                spotify_id: spotify_id.clone(),
                youtube_music_id: youtube_music_id.clone(),
                moment_seconds: first.moment_seconds,
            }
        } else {
            v2::Content::DualLinks { first, second }
        }
    } else {
        v2::Content::DualLinks { first, second }
    };
    encoded_json(&v2::encode_adaptive(&content).map_err(js_error)?)
}

/// Validate the CRC/RS-protected frame before generating output geometry.
#[wasm_bindgen]
pub fn v2_render_svg(
    metadata: u8,
    symbols: &[u8],
    pitch: f64,
    height: f64,
    quiet: f64,
) -> Result<String, JsValue> {
    let frame = verified_frame(metadata, symbols)?;
    v2::visual::render_svg(&frame, profile(pitch, height, quiet)).map_err(js_error)
}

#[wasm_bindgen]
pub fn v2_render_png(
    metadata: u8,
    symbols: &[u8],
    pitch: f64,
    height: f64,
    quiet: f64,
) -> Result<Vec<u8>, JsValue> {
    let frame = verified_frame(metadata, symbols)?;
    v2::visual::render_png(&frame, profile(pitch, height, quiet)).map_err(js_error)
}

/// Override two visual-only distances in physical output pixels.
/// Neither value is written into the frame or needed by the scanner.
#[wasm_bindgen]
pub fn v2_render_svg_with_spacing(
    metadata: u8,
    symbols: &[u8],
    pitch: f64,
    height: f64,
    quiet: f64,
    payload_center_gap: f64,
    logo_payload_gap: f64,
) -> Result<String, JsValue> {
    let frame = verified_frame(metadata, symbols)?;
    v2::visual::render_svg_with_spacing(
        &frame,
        profile(pitch, height, quiet),
        v2::visual::VisualSpacing {
            payload_center_gap,
            logo_payload_gap,
        },
    )
    .map_err(js_error)
}

/// The same configurable layout as SVG, delivered as a raster PNG.
#[wasm_bindgen]
pub fn v2_render_png_with_spacing(
    metadata: u8,
    symbols: &[u8],
    pitch: f64,
    height: f64,
    quiet: f64,
    payload_center_gap: f64,
    logo_payload_gap: f64,
) -> Result<Vec<u8>, JsValue> {
    let frame = verified_frame(metadata, symbols)?;
    v2::visual::render_png_with_spacing(
        &frame,
        profile(pitch, height, quiet),
        v2::visual::VisualSpacing {
            payload_center_gap,
            logo_payload_gap,
        },
    )
    .map_err(js_error)
}

/// Independently size W while retaining the exact payload scale and gaps.
/// Height units are output pixels; no scale parameter enters frame metadata.
#[wasm_bindgen]
#[allow(clippy::too_many_arguments)]
pub fn v2_render_svg_with_geometry(
    metadata: u8,
    symbols: &[u8],
    pitch: f64,
    payload_height: f64,
    quiet: f64,
    payload_center_gap: f64,
    logo_payload_gap: f64,
    logo_height: f64,
) -> Result<String, JsValue> {
    let frame = verified_frame(metadata, symbols)?;
    v2::visual::render_svg_with_spacing_and_logo_height(
        &frame,
        profile(pitch, payload_height, quiet),
        v2::visual::VisualSpacing {
            payload_center_gap,
            logo_payload_gap,
        },
        logo_height,
    )
    .map_err(js_error)
}

#[wasm_bindgen]
#[allow(clippy::too_many_arguments)]
pub fn v2_render_png_with_geometry(
    metadata: u8,
    symbols: &[u8],
    pitch: f64,
    payload_height: f64,
    quiet: f64,
    payload_center_gap: f64,
    logo_payload_gap: f64,
    logo_height: f64,
) -> Result<Vec<u8>, JsValue> {
    let frame = verified_frame(metadata, symbols)?;
    v2::visual::render_png_with_spacing_and_logo_height(
        &frame,
        profile(pitch, payload_height, quiet),
        v2::visual::VisualSpacing {
            payload_center_gap,
            logo_payload_gap,
        },
        logo_height,
    )
    .map_err(js_error)
}

/// Render a named visual style without storing style data in the encoded frame.
#[wasm_bindgen]
pub fn v2_render_svg_style(metadata: u8, symbols: &[u8], style: &str) -> Result<String, JsValue> {
    let frame = verified_frame(metadata, symbols)?;
    v2::visual::render_svg_style(&frame, visual_style(style)?).map_err(js_error)
}

/// Render an optically equivalent PNG from the same bytes in either preset.
#[wasm_bindgen]
pub fn v2_render_png_style(metadata: u8, symbols: &[u8], style: &str) -> Result<Vec<u8>, JsValue> {
    let frame = verified_frame(metadata, symbols)?;
    v2::visual::render_png_style(&frame, visual_style(style)?).map_err(js_error)
}

#[wasm_bindgen]
pub fn v2_decode_frame(metadata: u8, symbols: &[u8]) -> Result<String, JsValue> {
    let decoded = v2::decode(&v2::Frame {
        metadata,
        symbols: symbols.to_vec(),
    })
    .map_err(js_error)?;
    serde_json::to_string(&decoded_value(&decoded).map_err(js_error)?).map_err(js_error)
}

/// Return decoded URLs and the W-local orientation in image pixels.
/// Reads a sufficiently isolated V2 mark; this is not a general multi-code finder.
#[wasm_bindgen]
pub fn v2_scan_image(image: &[u8]) -> Result<String, JsValue> {
    if image.len() > MAX_IMAGE_BYTES {
        return Err(js_error("image is too large"));
    }
    let located = v2::raster::scan_png_or_jpeg(image).map_err(js_error)?;
    located_json(located)
}

/// Recover a slanted/keystoned view using one bounded perspective hypothesis.
/// The browser rotates through candidates over successive camera frames.
#[wasm_bindgen]
pub fn v2_scan_image_oblique(image: &[u8], candidate: u32) -> Result<String, JsValue> {
    if image.len() > MAX_IMAGE_BYTES {
        return Err(js_error("image is too large"));
    }
    let located =
        v2::raster::scan_png_or_jpeg_oblique(image, candidate as usize).map_err(js_error)?;
    located_json(located)
}

/// Scan the original frame once, then check at most four nearby angle
/// hypotheses without re-parsing PNG/JPEG or re-running the original finder.
#[wasm_bindgen]
pub fn v2_scan_image_oblique_batch(
    image: &[u8],
    start_candidate: u32,
    count: u32,
) -> Result<String, JsValue> {
    if image.len() > MAX_IMAGE_BYTES {
        return Err(js_error("image is too large"));
    }
    let located = v2::raster::scan_png_or_jpeg_oblique_batch(
        image,
        start_candidate as usize,
        count.min(4) as usize,
    )
    .map_err(js_error)?;
    located_json(located)
}

fn located_json(located: v2::raster::Located) -> Result<String, JsValue> {
    let mut result = decoded_value(&located.decoded).map_err(js_error)?;
    let corners = [(0.0, 0.0), (1.0, 0.0), (0.0, 1.0), (1.0, 1.0)]
        .iter()
        .map(|&(x, y)| {
            let point = located
                .pose
                .project(x, y)
                .ok_or(wavecode::Error::InvalidGeometry)?;
            Ok(json!({ "x": point.x, "y": point.y }))
        })
        .collect::<Result<Vec<Value>, wavecode::Error>>()
        .map_err(js_error)?;
    let left = located
        .pose
        .project(0.0, 0.5)
        .ok_or_else(|| js_error("invalid W anchor origin"))?;
    let right = located
        .pose
        .project(1.0, 0.5)
        .ok_or_else(|| js_error("invalid W anchor direction"))?;
    let dx = right.x - left.x;
    let dy = right.y - left.y;
    let norm = dx.hypot(dy);
    if !norm.is_finite() || norm < 1.0e-7 {
        return Err(js_error("degenerate W anchor direction"));
    }
    result["version"] = json!(located.version);
    result["metadata"] = json!(located.profile);
    result["anchor_corrected_bits"] = json!(located.corrected_anchor_bits);
    result["bounds"] = json!({
        "left": located.bounds.left,
        "top": located.bounds.top,
        "width": located.bounds.width,
        "height": located.bounds.height,
    });
    result["anchor"] = json!({
        "corners": corners,
        "center": { "x": left.x, "y": left.y },
        "towards_payload": {
            "x": dx / norm,
            "y": dy / norm,
            "angle_radians": dy.atan2(dx),
        },
    });
    serde_json::to_string(&result).map_err(js_error)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn web_frame_contains_lossless_two_link_content() {
        let content = v2::Content::Dual {
            spotify_id: "0VjIjW4GlUZAMYd2vXMi3b".to_owned(),
            youtube_music_id: "JGwWNGJdvx8".to_owned(),
            moment_seconds: Some(59),
        };
        let frame = v2::encode(&content).unwrap();
        let object: Value = serde_json::from_str(&encoded_json(&frame).unwrap()).unwrap();
        assert_eq!(object["layout"], "dual");
        assert_eq!(object["symbols"].as_array().unwrap().len(), 33);
        let decoded = v2::decode(&frame).unwrap();
        let details = decoded_value(&decoded).unwrap();
        assert_eq!(details["urls"].as_array().unwrap().len(), 2);
        assert_eq!(
            details["urls"][1],
            "https://music.youtube.com/watch?v=JGwWNGJdvx8&t=59"
        );
    }

    #[test]
    fn web_style_presets_do_not_modify_binary_frames() {
        let content = v2::Content::Single(
            MusicCode::from_url("https://music.youtube.com/watch?v=JGwWNGJdvx8").unwrap(),
        );
        let frame = v2::encode(&content).unwrap();
        let first = v2_render_svg_style(frame.metadata, &frame.symbols, "classic").unwrap();
        let second = v2_render_svg_style(frame.metadata, &frame.symbols, "compact").unwrap();
        assert!(first.contains("class=\"data-upper\""));
        assert!(second.contains("class=\"data-upper\""));
        assert_ne!(first, second);
        assert_eq!(v2::decode(&frame).unwrap().content, content);
    }

    #[test]
    fn custom_spacing_changes_svg_geometry_without_mutating_frame() {
        let content = v2::Content::Single(
            MusicCode::from_url("https://music.youtube.com/watch?v=JGwWNGJdvx8").unwrap(),
        );
        let frame = v2::encode(&content).unwrap();
        let first = v2_render_svg_with_spacing(
            frame.metadata,
            &frame.symbols,
            18.0,
            210.0,
            18.0,
            4.0,
            200.0,
        )
        .unwrap();
        let second = v2_render_svg_with_spacing(
            frame.metadata,
            &frame.symbols,
            18.0,
            210.0,
            18.0,
            80.0,
            4.0,
        )
        .unwrap();
        assert_ne!(first, second);
        assert!(first.contains("data-version=\"2\""));
        assert_eq!(v2::decode(&frame).unwrap().content, content);
    }

    #[test]
    fn browser_frame_checks_checksum_before_rendering() {
        let content = v2::Content::Single(
            MusicCode::from_url("https://music.youtube.com/watch?v=JGwWNGJdvx8").unwrap(),
        );
        let mut frame = v2::encode(&content).unwrap();
        assert!(verified_frame(frame.metadata, &frame.symbols).is_ok());
        frame.metadata |= 2;
        assert!(v2::decode(&frame).is_err());
    }
}
