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
//! Measured on WeasyPrint 69: 12.000 / 0.000 / 0.000 / 24.000 / 24.000 pt,
//! which fulgur now reproduces exactly. Before the fix every row read
//! 24.000 pt — the "no value of the property does anything" symptom stated
//! as a number.

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
