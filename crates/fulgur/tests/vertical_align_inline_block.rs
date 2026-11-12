//! `vertical-align` must actually move an atomic inline box.
//!
//! Parley has no `vertical-align` and no strut: it puts every inline box's
//! bottom edge on the line's baseline and folds the box's whole height into
//! the line's ascent (`parley-0.6.0 layout/line/greedy.rs:486`). fulgur used
//! to consume that placement verbatim, so `middle`, `top`, `bottom` and
//! `baseline` all produced byte-identical output and the error against a
//! reference engine grew with the box — +3.67 mm at a 30 pt box in
//! `paperworx-repros/10-vertical-align-inline-block.html`.
//!
//! ## Why a *growth signature* rather than coordinates
//!
//! Absolute baselines bake in the reference engine's font metrics (fulgur
//! resolves Helvetica where WeasyPrint 69 picks Verdana on the same host, a
//! constant 0.7 mm offset on every line). What survives any font is how the
//! row's text baseline responds when only the inline box's *height* changes:
//! the box's own height, the strut's ascent and the x-height all cancel out
//! of the difference.
//!
//! Doubling a 24 pt box to 48 pt must move the row's text baseline by:
//!
//! | `vertical-align` | baseline moves by | why |
//! |---|---|---|
//! | `middle`    | **half** the growth (12 pt) | box straddles `baseline - x-height/2` |
//! | `top`       | **nothing**                 | box hangs from the line's top edge |
//! | `text-top`  | **nothing**                 | box hangs from `baseline - ascent` |
//! | `bottom`    | the full growth (24 pt)     | box hangs from the line's bottom edge |
//! | `baseline`  | the full growth (24 pt)     | empty box has no baseline, so its bottom margin edge sits on the baseline |
//!
//! Measured on **this document**, WeasyPrint 69 gives 12.000 / 0.000 / 0.000
//! / 24.000 / 24.000 pt and fulgur reproduces it exactly. Before the fix
//! every row read 24.000 pt — the "no value of the property does anything"
//! symptom stated as a number.
//!
//! `bottom` is the one row not to generalise from. Both boxes on the line
//! carry the same `vertical-align`, so with `bottom` there is no flow-aligned
//! box to set the line's bottom edge and the strut sets it instead; the tall
//! box then grows the line box *upwards* and drags the row down with it. On
//! other shapes the two references part company here — Chrome 151 reports the
//! full growth where WeasyPrint reports none — so `bottom` is a
//! references-disagree case in the sense CLAUDE.md uses for the margin-box
//! box model. fulgur follows Chrome; do not "correct" it toward WeasyPrint on
//! the strength of this file alone.

use fulgur::Engine;
use fulgur::inspect::inspect;

const PAGE_H_PT: f32 = 841.89; // A4

/// One row holding a text inline-block and an empty, explicitly sized one,
/// followed by a plain block. The empty box is deliberate: it has no in-flow
/// line box, so CSS 2.1 §10.8.1 makes its *bottom margin edge* the baseline
/// it contributes, which is the case Parley's model already gets right and
/// therefore the strictest control for the others.
fn doc(vertical_align: &str, box_height_pt: f32) -> String {
    format!(
        "<!doctype html><html><head><style>\
         @page {{ size: A4; margin: 20mm }}\
         body {{ margin: 0; font: 9pt/1.15 sans-serif }}\
         p {{ margin: 0 }}\
         #row > p, #row > b {{ display: inline-block; \
         vertical-align: {vertical_align}; margin: 0 }}\
         #row > b {{ width: 100pt; height: {box_height_pt}pt }}\
         </style></head><body>\
         <div id=\"row\"><p>LEFT</p><b></b></div>\
         <p>AFTER</p>\
         </body></html>"
    )
}

/// `(row text baseline, following block's baseline)`, both in pt from the
/// page top. The row is the first thing in the body, so its *top* edge is
/// identical across box heights and the two numbers isolate, respectively,
/// placement inside the line box and the block flow around it.
fn baselines(vertical_align: &str, box_height_pt: f32) -> (f32, f32) {
    let pdf = Engine::builder()
        .build()
        .render(&doc(vertical_align, box_height_pt))
        .expect("render must succeed");
    let dir = tempfile::tempdir().expect("create tempdir");
    let path = dir.path().join("out.pdf");
    std::fs::write(&path, &pdf).expect("write pdf");
    let r = inspect(&path).expect("inspect must succeed");

    // `inspect` cannot recover the glyphs' text (lopdf does not read krilla's
    // ToUnicode CMap), but the positions are sound and there are exactly two
    // runs: the row's `LEFT`, then `AFTER` below it.
    let mut ys: Vec<f32> = r
        .text_items
        .iter()
        .filter(|t| t.page == 1)
        .map(|t| PAGE_H_PT - t.y)
        .collect();
    ys.sort_by(|a, b| a.partial_cmp(b).expect("finite coordinates"));
    assert_eq!(ys.len(), 2, "expected exactly two text runs on page 1");
    (ys[0], ys[1])
}

/// How far the row's text baseline moves when the box grows 24 pt → 48 pt.
fn baseline_growth(vertical_align: &str) -> f32 {
    let small = baselines(vertical_align, 24.0);
    let large = baselines(vertical_align, 48.0);
    // The block flow below must always absorb the full box growth: the line
    // box is as tall as the box in every alignment, so this is the invariant
    // that says the fix moved content *inside* the line and nothing else.
    assert!(
        (large.1 - small.1 - 24.0).abs() < 0.05,
        "{vertical_align}: the following block should move by the full 24pt, \
         moved by {:.3}",
        large.1 - small.1
    );
    large.0 - small.0
}

#[test]
fn middle_moves_the_baseline_by_half_the_box_growth() {
    let growth = baseline_growth("middle");
    assert!(
        (growth - 12.0).abs() < 0.05,
        "expected 12pt (half of 24pt), got {growth:.3}"
    );
}

#[test]
fn top_pins_the_baseline_against_the_line_top() {
    let growth = baseline_growth("top");
    assert!(
        growth.abs() < 0.05,
        "expected the baseline not to move, got {growth:.3}"
    );
}

#[test]
fn text_top_pins_the_baseline_against_the_font_ascent() {
    let growth = baseline_growth("text-top");
    assert!(
        growth.abs() < 0.05,
        "expected the baseline not to move, got {growth:.3}"
    );
}

#[test]
fn bottom_moves_the_baseline_by_the_full_box_growth() {
    let growth = baseline_growth("bottom");
    assert!(
        (growth - 24.0).abs() < 0.05,
        "expected the full 24pt, got {growth:.3}"
    );
}

/// The control. An empty inline-block has no baseline of its own, so
/// `baseline` legitimately behaves like `bottom` here — this is the one value
/// whose *growth* the old code already had right, and it must stay right.
#[test]
fn baseline_keyword_moves_the_baseline_by_the_full_box_growth() {
    let growth = baseline_growth("baseline");
    assert!(
        (growth - 24.0).abs() < 0.05,
        "expected the full 24pt, got {growth:.3}"
    );
}

/// The defect stated directly: fulgur produced identical output for every
/// value of the property. Four distinct growth signatures is the property
/// that cannot be satisfied by ignoring `vertical-align`.
#[test]
fn the_property_value_changes_the_layout() {
    let middle = baseline_growth("middle");
    let top = baseline_growth("top");
    let bottom = baseline_growth("bottom");
    assert!(
        (middle - top).abs() > 1.0 && (bottom - middle).abs() > 1.0,
        "middle/top/bottom must place a tall inline-block differently; \
         got {middle:.3} / {top:.3} / {bottom:.3}"
    );
}

// ── `margin-top` on an atomic inline box ──────────────────────────────────
//
// Parley's inline box is the box's *margin* box — `blitz-dom-0.2.4
// layout/inline.rs:57` builds its height as `margin.top + margin.bottom +
// content` — but every position fulgur records for the node itself is its
// *border* box. Reading a border-box baseline offset against a margin-box
// height slides a margined inline-block by its own `margin-top`.
//
// The shape is `crates/fulgur-vrt/fixtures/layout/review_card_inline_block.html`:
// a small-font inline-block chip carrying `margin-top`, alone on its line
// inside a larger-font card. Measured there against both references, which
// agree with each other to 0.8pt.

/// A chip with `margin-top: {margin_top_pt}` alone on its line. Sizes are
/// chosen so the chip's *margin* box is taller than the card's strut at both
/// margins under test — otherwise the strut, not the box, would set the line
/// box's top edge and the relations below would not hold.
fn margined_chip_doc(margin_top_pt: f32) -> String {
    format!(
        "<!doctype html><html><head><style>\
         @page {{ size: A4; margin: 20mm }}\
         body {{ margin: 0; font: 10pt/1.2 sans-serif }}\
         p, div {{ margin: 0 }}\
         #card > span {{ display: inline-block; padding: 1pt 7pt; \
         font-size: 7.5pt; line-height: 9pt; margin-top: {margin_top_pt}pt }}\
         </style></head><body>\
         <div id=\"card\"><div>BODY</div><span>CHIP</span></div>\
         <p>AFTER</p>\
         </body></html>"
    )
}

/// `(chip baseline, following block's baseline)`, pt from the page top.
fn chip_and_after(margin_top_pt: f32) -> (f32, f32) {
    let pdf = Engine::builder()
        .build()
        .render(&margined_chip_doc(margin_top_pt))
        .expect("render must succeed");
    let dir = tempfile::tempdir().expect("create tempdir");
    let path = dir.path().join("out.pdf");
    std::fs::write(&path, &pdf).expect("write pdf");
    let r = inspect(&path).expect("inspect must succeed");
    let mut ys: Vec<f32> = r
        .text_items
        .iter()
        .filter(|t| t.page == 1)
        .map(|t| PAGE_H_PT - t.y)
        .collect();
    ys.sort_by(|a, b| a.partial_cmp(b).expect("finite coordinates"));
    assert_eq!(ys.len(), 3, "expected BODY, CHIP and AFTER on page 1");
    (ys[1], ys[2])
}

/// `margin-top` sits entirely *above* the chip, so growing it must push the
/// chip down by exactly that much — no more, no less.
#[test]
fn margin_top_on_an_inline_block_moves_the_chip_by_its_own_growth() {
    let (chip_small, _) = chip_and_after(6.0);
    let (chip_large, _) = chip_and_after(18.0);
    let moved = chip_large - chip_small;
    assert!(
        (moved - 12.0).abs() < 0.05,
        "expected the chip to move by the full 12pt margin growth, moved by {moved:.3}"
    );
}

/// The other half of the same statement, and the half that survives any font:
/// nothing about `margin-top` belongs *below* the chip, so the distance from
/// the chip's baseline to the next block must not depend on it at all.
#[test]
fn margin_top_on_an_inline_block_leaves_the_space_below_it_alone() {
    let (chip_small, after_small) = chip_and_after(6.0);
    let (chip_large, after_large) = chip_and_after(18.0);
    let gap_small = after_small - chip_small;
    let gap_large = after_large - chip_large;
    assert!(
        (gap_large - gap_small).abs() < 0.05,
        "the gap below the chip must not move with margin-top; \
         got {gap_small:.3} at 6pt and {gap_large:.3} at 18pt"
    );
}

/// The inline-axis half of the same margin-box confusion, which only became
/// visible once the block axis was fixed: `crates/fulgur-vrt/fixtures/svg/
/// shapes.html` puts `margin: 40px` on an inline `<svg>`, and fulgur drew it
/// hard against the content edge on both axes. WeasyPrint 69 puts that box's
/// origin at (86.693, 86.693)pt on an A4/20mm page; fulgur now agrees to
/// 1e-5pt on both.
fn margin_left_chip_x(margin_left_pt: f32) -> f32 {
    let html = format!(
        "<!doctype html><html><head><style>\
         @page {{ size: A4; margin: 20mm }}\
         body {{ margin: 0; font: 10pt/1.2 sans-serif }}\
         div {{ margin: 0 }}\
         span {{ display: inline-block; margin-left: {margin_left_pt}pt }}\
         </style></head><body>\
         <div><span>CHIP</span></div>\
         </body></html>"
    );
    let pdf = Engine::builder()
        .build()
        .render(&html)
        .expect("render must succeed");
    let dir = tempfile::tempdir().expect("create tempdir");
    let path = dir.path().join("out.pdf");
    std::fs::write(&path, &pdf).expect("write pdf");
    let r = inspect(&path).expect("inspect must succeed");
    let xs: Vec<f32> = r
        .text_items
        .iter()
        .filter(|t| t.page == 1)
        .map(|t| t.x)
        .collect();
    assert_eq!(xs.len(), 1, "expected exactly one text run");
    xs[0]
}

#[test]
fn margin_left_on_an_inline_block_moves_the_box_by_its_own_growth() {
    let moved = margin_left_chip_x(30.0) - margin_left_chip_x(0.0);
    assert!(
        (moved - 30.0).abs() < 0.05,
        "expected the chip to move right by the full 30pt margin, moved by {moved:.3}"
    );
}
