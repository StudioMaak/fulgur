//! A page-margin box's own box is its slot's rect: a `background` set on it
//! fills the whole band, not just the strip its text happens to occupy.
//!
//! Measured with a red `background` on `@bottom-center`, A4/25mm, where the
//! band is x 25-185mm and y 272-297mm:
//!
//! | engine        | horizontal                 | vertical                  |
//! |---------------|----------------------------|---------------------------|
//! | WeasyPrint 69 | 103.0-105.8 (shrink-wrap)  | 272.3-296.3 (full band)   |
//! | fulgur        | 25.4-184.1  (full width)   | 282.9-285.8 (content!)    |
//! | Chrome 151    | 24.7-184.1  (full width)   | 272.3-296.3 (full band)   |
//!
//! This is the one case in this whole exercise where the two reference
//! engines disagree with each other, so their agreement cannot settle it.
//! Chrome's model — the margin box *is* its rect on both axes — is the one
//! matching CSS Paged Media 3 §5.3.3, and it also explains the `text-align`
//! divergence: WeasyPrint shrink-wraps horizontally, so aligning inside the
//! box is moot there.
//!
//! Asserted on the painted path rather than a raster: the fill is emitted as
//! a path in the content stream, so with compression off its coordinates can
//! be read directly and are font-independent.

use fulgur::Engine;
use fulgur::asset::AssetBundle;
use krilla::SerializeSettings;

const PAGE_H_PT: f32 = 841.89; // A4
const MARGIN_PT: f32 = 70.866_14; // 25mm

fn uncompressed() -> SerializeSettings {
    SerializeSettings {
        compress_content_streams: false,
        ascii_compatible: true,
        ..Default::default()
    }
}

/// Bounds of the painted path, as `(min_x, max_x, min_y, max_y)` in PDF
/// points with the origin at the page's bottom-left.
///
/// krilla draws page content under a `1 0 0 -1 0 <page_height> cm` flip, so
/// the `m` / `l` coordinates in the stream are y-down from the page top and
/// have to be converted back. Reading them beats rasterising: the numbers are
/// exact and carry no font dependence.
///
/// The fixture paints exactly one path — the margin box's background — so
/// every point collected belongs to it.
fn painted_path_bounds(pdf: &[u8]) -> Option<(f32, f32, f32, f32)> {
    let text = String::from_utf8_lossy(pdf);
    let mut pts: Vec<(f32, f32)> = Vec::new();
    for line in text.lines() {
        let t = line.trim();
        let Some(rest) = t.strip_suffix(" m").or_else(|| t.strip_suffix(" l")) else {
            continue;
        };
        let mut it = rest.split_whitespace();
        if let (Some(a), Some(b), None) = (it.next(), it.next(), it.next())
            && let (Ok(x), Ok(y_down)) = (a.parse::<f32>(), b.parse::<f32>())
        {
            pts.push((x, PAGE_H_PT - y_down));
        }
    }
    if pts.is_empty() {
        return None;
    }
    let xs: Vec<f32> = pts.iter().map(|p| p.0).collect();
    let ys: Vec<f32> = pts.iter().map(|p| p.1).collect();
    Some((
        xs.iter().cloned().fold(f32::INFINITY, f32::min),
        xs.iter().cloned().fold(f32::NEG_INFINITY, f32::max),
        ys.iter().cloned().fold(f32::INFINITY, f32::min),
        ys.iter().cloned().fold(f32::NEG_INFINITY, f32::max),
    ))
}

fn render_with_background() -> Vec<u8> {
    let mut assets = AssetBundle::new();
    assets.add_css(
        "@page { size: A4; margin: 25mm; \
         @bottom-center { content: \"Xx\"; font-size: 8pt; background: #ff0000 } }",
    );
    Engine::builder()
        .assets(assets)
        .serialize_settings(uncompressed())
        .build()
        .render("<!doctype html><html><body><p>body</p></body></html>")
        .expect("render must succeed")
}

#[test]
fn a_margin_box_background_fills_its_band_vertically() {
    let pdf = render_with_background();
    let (_, _, min_y, max_y) =
        painted_path_bounds(&pdf).expect("the background must paint something");

    // The bottom band runs from the page bottom (y = 0) up to the margin.
    assert!(
        min_y < 1.0,
        "the background must reach the bottom edge of its band (y = 0), got {min_y:.2}pt"
    );
    assert!(
        (max_y - MARGIN_PT).abs() < 1.0,
        "the background must reach the top of its band ({MARGIN_PT:.2}pt), got {max_y:.2}pt \
         — painting only {:.2}pt means it covers the text's strip, not the box",
        max_y - min_y
    );
}

#[test]
fn a_margin_box_background_fills_its_band_horizontally() {
    let pdf = render_with_background();
    let (min_x, max_x, _, _) =
        painted_path_bounds(&pdf).expect("the background must paint something");

    // A lone box on an edge is given that edge's whole content-width band.
    assert!(
        (min_x - MARGIN_PT).abs() < 2.0,
        "the background must start at the content's left edge, got {min_x:.2}pt"
    );
    assert!(
        (max_x - (595.28 - MARGIN_PT)).abs() < 2.0,
        "the background must reach the content's right edge, got {max_x:.2}pt"
    );
}
