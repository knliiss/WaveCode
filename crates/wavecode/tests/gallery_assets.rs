use wavecode::v1::{self, RenderProfile};
use wavecode::MusicCode;

const GALLERY: [(&str, &str, &str); 3] = [
    (
        "https://open.spotify.com/track/0VjIjW4GlUZAMYd2vXMi3b",
        "blinding-lights",
        include_str!("../../../assets/examples/blinding-lights.svg"),
    ),
    (
        "https://open.spotify.com/track/7qiZfU4dY1lWllzX7mPBI3",
        "shape-of-you",
        include_str!("../../../assets/examples/shape-of-you.svg"),
    ),
    (
        "https://open.spotify.com/track/2Fxmhks0bxGSBdJ92vM42m",
        "bad-guy",
        include_str!("../../../assets/examples/bad-guy.svg"),
    ),
];

fn attribute(line: &str, key: &str) -> f64 {
    let prefix = format!("{key}=\"");
    let start = line.find(&prefix).unwrap() + prefix.len();
    let end = line[start..].find('"').unwrap() + start;
    line[start..end].parse::<f64>().unwrap()
}

#[test]
fn committed_gallery_is_generated_from_real_track_identities() {
    for (url, name, stored_svg) in GALLERY {
        let track = MusicCode::from_url(url).unwrap();
        let encoded = v1::encode(&track).unwrap();
        let generated =
            v1::render_svg_with_profile(&encoded, RenderProfile::new(960.0, 240.0)).unwrap();
        assert_eq!(generated, stored_svg, "gallery SVG drift: {name}");

        let decoded = v1::decode_svg(stored_svg).unwrap();
        assert_eq!(decoded.decoded.code, track, "{name}");
        assert_eq!(decoded.decoded.corrected_symbols, 0, "{name}");
    }
}

#[test]
fn v1_defaults_to_true_capsule_endcaps() {
    let profile = RenderProfile::default();
    assert_eq!(profile.corner_radius_fraction, 0.5);

    let track = MusicCode::from_url(GALLERY[0].0).unwrap();
    let symbols = v1::encode(&track).unwrap();
    let svg = v1::render_svg(&symbols).unwrap();
    let first_rail = svg
        .lines()
        .find(|line| line.contains("class=\"wavecode-upper\""))
        .unwrap();

    let width = attribute(first_rail, "width");
    let radius = attribute(first_rail, "rx");
    assert!((radius * 2.0 - width).abs() < 0.000_002);
    assert_eq!(v1::decode_svg(&svg).unwrap().decoded.code, track);
}

#[cfg(feature = "raster")]
#[test]
fn capsule_png_is_still_optically_decodable() {
    let profile = RenderProfile::new(960.0, 240.0);
    for (url, name, _) in GALLERY {
        let track = MusicCode::from_url(url).unwrap();
        let symbols = v1::encode(&track).unwrap();
        let raster = v1::render_png(&symbols, profile).unwrap();
        assert_eq!(
            v1::scan_image(&raster).unwrap().decoded.code,
            track,
            "{name}"
        );
    }
}
