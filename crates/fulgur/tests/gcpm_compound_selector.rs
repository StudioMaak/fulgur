//! A GCPM construct must be honoured whichever *selector shape* declares it.
//!
//! `GcpmSheetParser` used to accept only a single bare tag / class / id, and
//! dropped every other prelude silently. A theme that writes
//! `section[data-section="hdr"] { position: running(hdr) }` — the shape real
//! documents use — therefore had its running element neither harvested into
//! the margin box nor taken out of the flow: it printed in the body and
//! displaced everything after it, on every page it appeared.
//!
//! The tests assert the property rather than coordinates: the selector shape
//! must not change the layout. Measured against WeasyPrint 69, whose output
//! for the two shapes is identical.

use fulgur::Engine;
use fulgur::inspect::inspect;

const PAGE_H_PT: f32 = 841.89; // A4
const MARGIN_PT: f32 = 70.866_14; // 25mm

const BODY: &str = "<section data-role=\"hdr\" class=\"hdr\" id=\"hdr\">RUNHEAD</section>\
                    <p>page one</p><div style=\"break-before: page\"></div><p>page two</p>";

/// `(pages, runs drawn in a margin band, runs drawn in the body area, body ys)`.
fn layout(selector: &str) -> (u32, usize, usize, Vec<i32>) {
    let css = format!(
        "@page {{ size: A4; margin: 25mm; @top-center {{ content: element(rh) }} }}\n\
         {selector} {{ position: running(rh); font-size: 9pt }}"
    );
    let html =
        format!("<!doctype html><html><head><style>{css}</style></head><body>{BODY}</body></html>");
    let pdf = Engine::builder()
        .build()
        .render(&html)
        .expect("render must succeed");

    let dir = tempfile::tempdir().expect("create tempdir");
    let path = dir.path().join("out.pdf");
    std::fs::write(&path, &pdf).expect("write pdf");
    let r = inspect(&path).expect("inspect must succeed");

    let from_top = |y: f32| PAGE_H_PT - y;
    let band = r
        .text_items
        .iter()
        .filter(|t| from_top(t.y) < MARGIN_PT)
        .count();
    let body: Vec<i32> = r
        .text_items
        .iter()
        .filter(|t| from_top(t.y) >= MARGIN_PT)
        .map(|t| (from_top(t.y) * 100.0).round() as i32)
        .collect();
    (r.pages, band, body.len(), body)
}

/// The control — a bare class selector was always supported.
#[test]
fn a_class_selected_running_element_leaves_the_flow() {
    let (pages, band, body, _) = layout(".hdr");
    assert_eq!(pages, 2, "the document should span two pages");
    assert_eq!(band, 2, "the running header belongs on both pages");
    assert_eq!(body, 2, "only the two body paragraphs stay in the flow");
}

#[test]
fn an_attribute_selected_running_element_leaves_the_flow() {
    let (pages, band, body, _) = layout("section[data-role=\"hdr\"]");
    assert_eq!(pages, 2, "the document should span two pages");
    assert_eq!(band, 2, "the running header belongs on both pages");
    assert_eq!(
        body, 2,
        "the source element must not remain in the body flow"
    );
}

#[test]
fn a_compound_tag_and_class_selected_running_element_leaves_the_flow() {
    let (pages, band, body, _) = layout("section.hdr");
    assert_eq!(pages, 2, "the document should span two pages");
    assert_eq!(band, 2, "the running header belongs on both pages");
    assert_eq!(
        body, 2,
        "the source element must not remain in the body flow"
    );
}

/// The property the defect actually broke: the shape of the selector is not
/// allowed to move a single glyph.
#[test]
fn the_selector_shape_does_not_change_the_layout() {
    let reference = layout(".hdr");
    for selector in [
        "section[data-role=\"hdr\"]",
        "section.hdr",
        "[data-role]",
        "section#hdr",
        "section[data-role~=\"hdr\"].hdr#hdr",
    ] {
        assert_eq!(
            layout(selector),
            reference,
            "`{selector}` must lay out exactly as `.hdr` does"
        );
    }
}
