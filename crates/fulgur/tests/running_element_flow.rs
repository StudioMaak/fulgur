//! `position: running(<name>)` must take the element out of the document
//! flow, whichever way its CSS reached the document.
//!
//! The rewrite that turns `position: running(x)` into `display: none` is
//! applied while building `cleaned_css`, and an inline `<style>` block's text
//! is never rewritten that way — so a running element declared inline was
//! harvested into the margin box *and* left in the body, printing twice and
//! displacing everything after it. A themed document ships its CSS in a
//! `<style>` block, so this is the path real documents take.
//!
//! The tests assert the delivery-independence directly: the same CSS through
//! an `AssetBundle` and through an inline `<style>` must lay out identically.

use fulgur::Engine;
use fulgur::asset::AssetBundle;
use fulgur::inspect::inspect;

const PAGE_H_PT: f32 = 841.89; // A4
const MARGIN_PT: f32 = 70.866_14; // 25mm

const CSS: &str = "@page { size: A4; margin: 25mm; @top-center { content: element(rh) } }\n\
                   .rh { position: running(rh); font-size: 9pt }";
const BODY: &str = "<section class=\"rh\">RUNHEAD</section><p>page one</p>\
                    <div style=\"break-before: page\"></div><p>page two</p>";

/// `(pages, runs drawn in a margin band, runs drawn in the body area)`.
fn layout(css_inline: bool) -> (u32, usize, usize) {
    let (html, assets) = if css_inline {
        (
            format!(
                "<!doctype html><html><head><style>{CSS}</style></head><body>{BODY}</body></html>"
            ),
            None,
        )
    } else {
        let mut a = AssetBundle::new();
        a.add_css(CSS);
        (
            format!("<!doctype html><html><body>{BODY}</body></html>"),
            Some(a),
        )
    };
    let mut builder = Engine::builder();
    if let Some(a) = assets {
        builder = builder.assets(a);
    }
    let pdf = builder.build().render(&html).expect("render must succeed");

    let dir = tempfile::tempdir().expect("create tempdir");
    let path = dir.path().join("out.pdf");
    std::fs::write(&path, &pdf).expect("write pdf");
    let r = inspect(&path).expect("inspect must succeed");

    let from_top = |y: f32| PAGE_H_PT - y;
    let in_band = r
        .text_items
        .iter()
        .filter(|t| from_top(t.y) < MARGIN_PT)
        .count();
    let in_body = r
        .text_items
        .iter()
        .filter(|t| from_top(t.y) >= MARGIN_PT)
        .count();
    (r.pages, in_band, in_body)
}

/// The control — through an `AssetBundle` the rewrite happens and the source
/// element leaves the flow.
#[test]
fn a_running_element_from_bundled_css_leaves_the_flow() {
    let (pages, band, body) = layout(false);
    assert_eq!(pages, 2, "the document should span two pages");
    assert_eq!(band, 2, "the running header belongs on both pages");
    assert_eq!(
        body, 2,
        "only the two body paragraphs should remain in the flow"
    );
}

/// The defect: declared in an inline `<style>`, the element was harvested
/// into the margin box but never removed from the body.
#[test]
fn a_running_element_from_an_inline_style_also_leaves_the_flow() {
    let (pages, band, body) = layout(true);
    assert_eq!(pages, 2, "the document should span two pages");
    assert_eq!(band, 2, "the running header belongs on both pages");
    assert_eq!(
        body, 2,
        "the running element's source must not also render in the body — \
         a third body run is the source element printing a second time"
    );
}

/// Stated as the general property, so a future divergence in either
/// direction fails rather than only the case we happened to hit.
#[test]
fn how_the_css_is_delivered_does_not_change_the_layout() {
    assert_eq!(
        layout(true),
        layout(false),
        "an inline <style> and an AssetBundle carrying the same CSS must \
         produce the same (pages, margin-band runs, body runs)"
    );
}
