//! CSS Paged Media 3 §5.3.2: every page-margin box has a default
//! `text-align` / `vertical-align` decided by which slot it occupies.
//!
//! Without them a box renders flush to the top-left of its rect. That is
//! invisible for `@top-left`, whose rect starts at the content's left edge
//! anyway, and badly wrong for every other slot: a lone `@bottom-right`
//! spans the whole content width, so its footer landed at the *left*
//! margin, on the paper edge.
//!
//! The expected geometry here was measured, not recalled — WeasyPrint 69
//! and Chrome 151 headless-shell were rendered on identical input and
//! agreed on all sixteen slots to within glyph-bearing noise.
//!
//! Every assertion below is an *anchor invariant*: a relation that holds
//! whatever the font's glyph widths turn out to be. `inspect` reports a
//! text item's `width` as a crude `chars × font_size` estimate (32pt for a
//! run that measures 24pt), so any assertion that leaned on the drawn text's
//! extent would be testing that estimate rather than the layout.

use fulgur::Engine;
use fulgur::asset::AssetBundle;
use fulgur::inspect::inspect;

const PAGE_H_PT: f32 = 841.89; // A4
const PAGE_W_PT: f32 = 595.28;
const MARGIN_PT: f32 = 70.866_14; // 25mm
const CONTENT_LEFT: f32 = MARGIN_PT;
const CONTENT_RIGHT: f32 = PAGE_W_PT - MARGIN_PT;
const CONTENT_TOP: f32 = MARGIN_PT;
const CONTENT_BOTTOM: f32 = PAGE_H_PT - MARGIN_PT;

/// Render a document whose only margin box is `slot`, and return that box's
/// drawn text position as `(x, y)` in PDF points (origin bottom-left).
///
/// One slot at a time on purpose: alone on its edge a box is given that
/// edge's whole band, so where the glyphs sit inside the band names the
/// alignment directly, with none of §5.3.3's width distribution mixed in.
///
/// The body is deliberately empty and the box's text is set at 8pt so it can
/// be told apart from any body text by size alone.
fn margin_box_text_pos(slot: &str) -> (f32, f32) {
    margin_box_text_pos_with(slot, "")
}

/// As [`margin_box_text_pos`], with `extra` appended to the box's own
/// declarations — used to check that an author's properties override the
/// §5.3.2 defaults rather than being ignored.
fn margin_box_text_pos_with(slot: &str, extra: &str) -> (f32, f32) {
    let mut assets = AssetBundle::new();
    assets.add_css(format!(
        "@page {{ size: A4; margin: 25mm; @{slot} {{ content: \"Xx\"; font-size: 8pt{extra} }} }}"
    ));
    let pdf = Engine::builder()
        .assets(assets)
        .build()
        .render("<!doctype html><html><body><p></p></body></html>")
        .expect("render must succeed");

    let dir = tempfile::tempdir().expect("create tempdir");
    let path = dir.path().join("out.pdf");
    std::fs::write(&path, &pdf).expect("write pdf");
    let result = inspect(&path).expect("inspect must succeed");

    let items: Vec<_> = result
        .text_items
        .iter()
        .filter(|t| (t.font_size - 8.0).abs() < 0.01)
        .collect();
    assert_eq!(
        items.len(),
        1,
        "expected exactly one 8pt run for @{slot}, got {items:#?}"
    );
    (items[0].x, items[0].y)
}

/// Distance from the top of the page down to the text's baseline.
fn from_page_top(y: f32) -> f32 {
    PAGE_H_PT - y
}

fn assert_close(actual: f32, expected: f32, what: &str) {
    assert!(
        (actual - expected).abs() < 0.5,
        "{what}: expected {expected:.2}pt, got {actual:.2}pt"
    );
}

/// Left / centre / right across the top band, without needing to know how
/// wide the drawn text is.
///
/// Alone on the edge each of the three boxes gets the same rect — the full
/// content width — so the same string has the same width `w` in all three.
/// That gives `left = L`, `centre = L + (avail - w)/2` and `right = L +
/// avail - w`, and therefore `right - left == 2 * (centre - left)` exactly,
/// with `w` cancelling out.
#[test]
fn top_band_boxes_anchor_left_centre_and_right() {
    let (x_left, _) = margin_box_text_pos("top-left");
    let (x_centre, _) = margin_box_text_pos("top-center");
    let (x_right, _) = margin_box_text_pos("top-right");

    assert_close(x_left, CONTENT_LEFT, "@top-left starts at the content edge");

    assert!(
        x_right > x_left + 1.0,
        "@top-right must not sit at the left margin like @top-left \
         (left={x_left:.2}pt, right={x_right:.2}pt) — this is the original defect"
    );
    assert!(
        x_centre > x_left + 1.0 && x_centre < x_right - 1.0,
        "@top-center must fall between the two (left={x_left:.2}, \
         centre={x_centre:.2}, right={x_right:.2})"
    );
    assert_close(
        x_right - x_left,
        2.0 * (x_centre - x_left),
        "centre sits exactly halfway between the left and right anchors",
    );
}

/// The bottom band anchors identically to the top band: same rule, same edge
/// geometry, mirrored vertically.
#[test]
fn bottom_band_boxes_anchor_left_centre_and_right() {
    let (x_left, _) = margin_box_text_pos("bottom-left");
    let (x_centre, _) = margin_box_text_pos("bottom-center");
    let (x_right, _) = margin_box_text_pos("bottom-right");

    assert_close(
        x_left,
        CONTENT_LEFT,
        "@bottom-left starts at the content edge",
    );
    assert!(
        x_right > x_left + 1.0,
        "@bottom-right landed at the left margin ({x_right:.2}pt) — the reported defect"
    );
    assert_close(
        x_right - x_left,
        2.0 * (x_centre - x_left),
        "centre sits exactly halfway between the left and right anchors",
    );
}

/// The corners align *toward* the page's content area rather than the paper
/// edge, so a left corner is right-aligned and a right corner left-aligned.
#[test]
fn corner_boxes_anchor_toward_the_content_area() {
    let (x_top_right_corner, _) = margin_box_text_pos("top-right-corner");
    assert_close(
        x_top_right_corner,
        CONTENT_RIGHT,
        "@top-right-corner is left-aligned, so it starts at the right band's inner edge",
    );

    // The left corner is right-aligned: its text *ends* at the content edge.
    // Its start therefore sits a glyph-width short of it — unknown here, but
    // strictly inside the 70.87pt band, and never at the paper edge.
    let (x_top_left_corner, _) = margin_box_text_pos("top-left-corner");
    assert!(
        x_top_left_corner > 1.0 && x_top_left_corner < CONTENT_LEFT,
        "@top-left-corner must be right-aligned within the left band, \
         not flush against the paper edge (got {x_top_left_corner:.2}pt)"
    );
}

/// Horizontal bands centre their content vertically — the band is the page
/// margin, and text hugging the paper edge is never what was meant.
///
/// Checked as a symmetry so no font metric is needed: the baseline's offset
/// *within its own band* must be the same for the top and bottom boxes,
/// since both bands are the same height and both are middle-aligned.
#[test]
fn top_and_bottom_boxes_centre_vertically_in_their_band() {
    let (_, y_top) = margin_box_text_pos("top-center");
    let (_, y_bottom) = margin_box_text_pos("bottom-center");

    // Top band runs from the page top down to CONTENT_TOP.
    let offset_in_top_band = from_page_top(y_top);
    // Bottom band's top edge is CONTENT_BOTTOM; `y` is already from the page
    // bottom, so the band-relative offset is simply the margin less `y`.
    let offset_in_bottom_band = MARGIN_PT - y_bottom;

    assert_close(
        offset_in_top_band,
        offset_in_bottom_band,
        "top and bottom margin boxes sit at the same offset within their band",
    );
    assert!(
        offset_in_top_band > MARGIN_PT * 0.3,
        "@top-center must be centred in its band, not flush to the paper edge \
         (baseline is {offset_in_top_band:.2}pt into a {MARGIN_PT:.2}pt band)"
    );
}

/// The side bands are the only ones whose slots are *named* by a vertical
/// position, so they are the only ones that align top and bottom.
#[test]
fn side_band_boxes_honour_their_named_vertical_slot() {
    let (_, y_top) = margin_box_text_pos("left-top");
    let (_, y_middle) = margin_box_text_pos("left-middle");
    let (_, y_bottom) = margin_box_text_pos("left-bottom");

    let top_from_page_top = from_page_top(y_top);
    let middle_from_page_top = from_page_top(y_middle);
    let bottom_from_page_top = from_page_top(y_bottom);

    // `@left-top` alone spans the whole content height, and aligns to its top.
    assert!(
        top_from_page_top > CONTENT_TOP && top_from_page_top < CONTENT_TOP + 20.0,
        "@left-top must start at the content area's top edge ({CONTENT_TOP:.2}pt), \
         got a baseline at {top_from_page_top:.2}pt"
    );
    assert!(
        bottom_from_page_top < CONTENT_BOTTOM && bottom_from_page_top > CONTENT_BOTTOM - 20.0,
        "@left-bottom must end at the content area's bottom edge ({CONTENT_BOTTOM:.2}pt), \
         got a baseline at {bottom_from_page_top:.2}pt"
    );

    // And the middle one is centred between them, so it is strictly ordered.
    assert!(
        middle_from_page_top > top_from_page_top && middle_from_page_top < bottom_from_page_top,
        "@left-middle must fall between @left-top and @left-bottom \
         (top={top_from_page_top:.2}, middle={middle_from_page_top:.2}, \
          bottom={bottom_from_page_top:.2})"
    );
    let content_centre = (CONTENT_TOP + CONTENT_BOTTOM) / 2.0;
    assert!(
        (middle_from_page_top - content_centre).abs() < 12.0,
        "@left-middle must be centred on the content area ({content_centre:.2}pt), \
         got {middle_from_page_top:.2}pt"
    );
}

/// The side bands are narrow, and every slot on them centres across the band
/// rather than picking a side.
#[test]
fn side_band_boxes_centre_across_their_narrow_band() {
    let (x_left_band, _) = margin_box_text_pos("left-middle");
    let (x_right_band, _) = margin_box_text_pos("right-middle");

    // Same string, so the same width `w` in both. Centred, the left band's
    // text starts at (margin - w)/2 and the right band's at
    // CONTENT_RIGHT + (margin - w)/2 — a difference of exactly CONTENT_RIGHT.
    assert_close(
        x_right_band - x_left_band,
        CONTENT_RIGHT,
        "left and right side boxes are centred in mirror-image bands",
    );
    assert!(
        x_left_band > 1.0,
        "@left-middle must be centred in its band, not flush to the paper edge \
         (got {x_left_band:.2}pt)"
    );
}

/// §5.3.2's alignment is a *default*, not a fixed behaviour: an author who
/// writes `vertical-align` in the at-rule must override it.
///
/// Both reference engines agree here (WeasyPrint 69 and Chrome 151 put
/// `vertical-align: bottom` on `@top-center` at 21.87mm / 21.83mm, against
/// 10.94mm for the default), which is what makes this a conformance gap
/// rather than a preference.
///
/// Checked without needing any font metric. The three alignments place the
/// content block at `0`, `(H - h) / 2` and `H - h` inside the box, so the
/// three baselines are *equally spaced* whatever `h` turns out to be —
/// middle sits exactly halfway between top and bottom.
#[test]
fn author_vertical_align_overrides_the_slot_default() {
    let (_, y_top) = margin_box_text_pos_with("top-center", "; vertical-align: top");
    let (_, y_middle) = margin_box_text_pos_with("top-center", "; vertical-align: middle");
    let (_, y_bottom) = margin_box_text_pos_with("top-center", "; vertical-align: bottom");

    let (top, middle, bottom) = (
        from_page_top(y_top),
        from_page_top(y_middle),
        from_page_top(y_bottom),
    );

    assert!(
        top < middle && middle < bottom,
        "author vertical-align must move the content \
         (top={top:.2}, middle={middle:.2}, bottom={bottom:.2}) — \
         equal values mean the declaration was ignored"
    );
    assert_close(
        middle - top,
        bottom - middle,
        "middle sits exactly halfway between top and bottom",
    );
    // `top` puts the content block flush to the band's top edge.
    assert!(
        top < MARGIN_PT * 0.3,
        "vertical-align:top must sit at the band's top edge, got {top:.2}pt"
    );
}

/// The default still applies when the author says nothing — overriding one
/// slot must not disturb the table.
#[test]
fn omitting_vertical_align_keeps_the_slot_default() {
    let (_, y_default) = margin_box_text_pos("top-center");
    let (_, y_middle) = margin_box_text_pos_with("top-center", "; vertical-align: middle");
    assert_close(
        from_page_top(y_default),
        from_page_top(y_middle),
        "@top-center defaults to vertical-align: middle",
    );
}
