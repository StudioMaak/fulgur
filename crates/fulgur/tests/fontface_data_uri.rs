//! An `@font-face` whose `src` is a `data:` URI must load.
//!
//! A theme ships its fonts that way so the rendered HTML is self-contained.
//! fulgur dropped them: `FulgurNetProvider` accepted only `file://`, so the
//! font was never fetched and the text silently fell back to a system font —
//! Chrome and WeasyPrint both embed the declared face.
//!
//! `data:` is the one non-`file://` scheme that cannot break offline-first:
//! the bytes are already inside the document, so serving it fetches nothing.
//!
//! Asserted with system fonts OFF and nothing registered, so the `@font-face`
//! is the only way any glyph can be drawn. That makes the test independent of
//! how the face ends up named in the PDF.

use fulgur::Engine;
use fulgur::inspect::inspect;
use std::path::PathBuf;

fn fixture_with_data_uri_font() -> String {
    let ttf = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/.fonts/NotoSans-Regular.ttf");
    let bytes = std::fs::read(ttf).expect("bundled Noto Sans must exist");
    // Minimal base64 (no dependency in the test crate).
    const T: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut b64 = String::new();
    for c in bytes.chunks(3) {
        let (b0, b1, b2) = (
            c[0] as u32,
            *c.get(1).unwrap_or(&0) as u32,
            *c.get(2).unwrap_or(&0) as u32,
        );
        let n = (b0 << 16) | (b1 << 8) | b2;
        b64.push(T[(n >> 18 & 63) as usize] as char);
        b64.push(T[(n >> 12 & 63) as usize] as char);
        b64.push(if c.len() > 1 {
            T[(n >> 6 & 63) as usize] as char
        } else {
            '='
        });
        b64.push(if c.len() > 2 {
            T[(n & 63) as usize] as char
        } else {
            '='
        });
    }
    format!(
        "<!doctype html><html><head><style>\
         @font-face {{ font-family: \"Embedded Probe\"; \
         src: url(data:font/ttf;base64,{b64}) format(\"truetype\"); }}\
         body {{ font-family: \"Embedded Probe\"; font-size: 12pt }}\
         </style></head><body><p>The quick brown fox jumps over the lazy dog.</p></body></html>"
    )
}

#[test]
#[ignore = "unimplemented: inline <style> never triggers Blitz's fetch_font_face; \
            see paperworx-repros/README.md item 8"]
fn a_font_face_with_a_data_uri_is_loaded() {
    let pdf = Engine::builder()
        .system_fonts(false) // nothing else can supply a glyph
        .build()
        .render(&fixture_with_data_uri_font())
        .expect("render must succeed");

    let dir = tempfile::tempdir().expect("create tempdir");
    let path = dir.path().join("out.pdf");
    std::fs::write(&path, &pdf).expect("write pdf");
    let runs = inspect(&path).expect("inspect").text_items.len();

    assert!(
        runs > 0,
        "the @font-face data: URI is the only font available, so nothing \
         drawn means it was never loaded"
    );
}
