//! A document must never render blank while a usable font is registered.
//!
//! When no registered family matches what the CSS asks for and the host has
//! no system fonts to fall back on, every glyph used to disappear — and the
//! render still reported success. For a document of record that is worse
//! than a hard error: the caller cannot tell "rendered correctly" from
//! "rendered nothing".
//!
//! This is not a WASM-only fault, though WASM is where it always bites
//! (a Worker isolate has no system fonts at all). It reproduces on any host
//! with `system_fonts(false)`, which is what these tests use.

use fulgur::Engine;
use fulgur::asset::AssetBundle;
use fulgur::inspect::inspect;
use std::path::PathBuf;

const SENTENCE: &str = "HELLO WORLD THIS IS A LINE OF TEXT";

fn noto() -> AssetBundle {
    let mut assets = AssetBundle::default();
    assets
        .add_font_file(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../examples/.fonts/NotoSans-Regular.ttf"),
        )
        .expect("bundled Noto Sans must load");
    assets
}

/// Render one paragraph in `family` and return how many text runs were drawn.
fn drawn_runs(family: &str, register_font: bool, system_fonts: bool) -> usize {
    let html = format!(
        "<!doctype html><html><body><p style=\"font-family: {family}\">{SENTENCE}</p></body></html>"
    );
    let mut builder = Engine::builder().system_fonts(system_fonts);
    if register_font {
        builder = builder.assets(noto());
    }
    let pdf = builder.build().render(&html).expect("render must succeed");

    let dir = tempfile::tempdir().expect("create tempdir");
    let path = dir.path().join("out.pdf");
    std::fs::write(&path, &pdf).expect("write pdf");
    inspect(&path)
        .expect("inspect must succeed")
        .text_items
        .len()
}

/// The control: the document asks for exactly what was registered.
#[test]
fn a_matching_family_renders() {
    assert!(
        drawn_runs("'Noto Sans'", true, false) > 0,
        "the registered family must render"
    );
}

/// The reported defect. `Georgia` is not registered and there are no system
/// fonts, so the whole paragraph silently vanished.
#[test]
fn an_unmatched_family_falls_back_to_a_registered_font() {
    assert!(
        drawn_runs("Georgia, serif", true, false) > 0,
        "a document naming an unregistered family must still render using \
         a registered font, not silently produce a blank page"
    );
}

/// Same, with no generic family in the list at all — there is nothing for a
/// generic-family mapping alone to catch, so this is the harder case.
#[test]
fn an_unmatched_family_with_no_generic_still_falls_back() {
    assert!(
        drawn_runs("'NoSuchFamily123'", true, false) > 0,
        "a font-family list naming only unregistered families must still render"
    );
}

/// Registering a font must not cost us the host's own fonts when they exist:
/// this is the path every desktop caller takes.
#[test]
fn system_fonts_still_resolve_when_available() {
    assert!(
        drawn_runs("Georgia, serif", false, true) > 0,
        "with system fonts enabled the host must still resolve the family"
    );
    assert!(
        drawn_runs("Georgia, serif", true, true) > 0,
        "registering a font must not suppress system font fallback"
    );
}

/// The fallback must be a real font, not a silently empty one: an unmatched
/// family should draw about as much as the matching one does.
#[test]
fn the_fallback_draws_the_same_text_as_a_matching_family() {
    let matched = drawn_runs("'Noto Sans'", true, false);
    let fell_back = drawn_runs("Georgia, serif", true, false);
    assert_eq!(
        fell_back, matched,
        "falling back must draw the same runs as the matching family, \
         not a partial or empty result"
    );
}

/// The reported repro, reduced: 40 paragraphs asking for a font stack none of
/// which is registered. It produced a single 1697-byte page holding no text at
/// all, while reporting success. Pagination is the sharper signal than glyph
/// count — text that does not render takes up no space, so the document
/// collapses to one page.
#[test]
fn a_multi_page_document_does_not_collapse_when_its_family_is_missing() {
    let paragraphs: String = (0..40)
        .map(|i| format!("<p>Paragraph {i}: {SENTENCE} {SENTENCE}</p>"))
        .collect();
    let render = |family: &str| {
        let html = format!(
            "<!doctype html><html><body style=\"font-family: {family}\">{paragraphs}</body></html>"
        );
        let pdf = Engine::builder()
            .system_fonts(false)
            .assets(noto())
            .build()
            .render(&html)
            .expect("render must succeed");
        let dir = tempfile::tempdir().expect("create tempdir");
        let path = dir.path().join("out.pdf");
        std::fs::write(&path, &pdf).expect("write pdf");
        let r = inspect(&path).expect("inspect must succeed");
        (r.pages, r.text_items.len())
    };

    let (matched_pages, matched_runs) = render("'Noto Sans'");
    let (missing_pages, missing_runs) = render("Georgia, 'Times New Roman', serif");

    assert!(
        matched_pages > 1,
        "control should span several pages, got {matched_pages}"
    );
    assert_eq!(
        (missing_pages, missing_runs),
        (matched_pages, matched_runs),
        "a document whose font-family matches nothing must lay out exactly as \
         the matching one does — collapsing to {missing_pages} page(s) with \
         {missing_runs} runs is the silent-blank defect"
    );
}
