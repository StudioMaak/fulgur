//! paperworx repro 12: the containing block of a `position: absolute`
//! element is the nearest **positioned** ancestor, or the initial
//! containing block (the page area) when there is none — CSS 2.1 §10.1.
//! A `position: static` wrapper is not a containing block, so wrapping an
//! absolutely-positioned block in one must not move it.
//!
//! Before the fix fulgur accepted Taffy's placement verbatim, and Taffy
//! resolves an absolute child's insets against its *immediate parent's*
//! box. That is only the CSS answer when the parent happens to be the
//! nearest positioned ancestor; one unstyled `<section>` in between was
//! enough to re-anchor `bottom: 0` onto the wrapper.
//!
//! Reference: WeasyPrint 69 puts the block in the same place with and
//! without the wrapper (`paperworx-repros/12-absolute-bottom-nested.html`
//! and its `12b-…-direct.html` control).

mod support;
use support::content_stream::text_matrix_ys;

use fulgur::{Engine, Margin, PageSize};

/// 300 × 400pt page, 20pt margins → a 260 × 360pt content area.
const PAGE_W: f32 = 300.0;
const PAGE_H: f32 = 400.0;
const MARGIN: f32 = 20.0;

/// Baselines this document produces, in pt from the page top (see
/// [`baselines`]). Measured against WeasyPrint 69, which agrees to the
/// usual ~1.4pt font-descriptor residual on every one of them.
const FLOW: f32 = 28.25;
const MID: f32 = 140.0;
/// `bottom: 0` — content-area bottom (20 + 360) less one 12pt line box,
/// plus the baseline's 8.25pt offset inside that line box.
const PINNED_BOTTOM: f32 = 376.25;
/// Static position: one 12pt line below `MID`.
const STATIC_POS: f32 = 152.0;

const HEAD: &str = r#"<!doctype html><html><head><style>
@page { size: 300pt 400pt; margin: 20pt; }
body { margin: 0; font-size: 10pt; line-height: 12pt; }
p { margin: 0; }
#pin { position: absolute; }
#filler { margin-top: 100pt; }
</style></head><body>"#;

const TAIL: &str = "</body></html>";

/// The three-element body every fixture shares. `pin_style` is appended to
/// the absolutely-positioned block's inline style.
fn inner(pin_style: &str) -> String {
    format!(
        r#"<p>FLOW</p>
<section id="filler"><p>MID</p></section>
<section id="pin" style="{pin_style}"><p>PIN</p></section>"#
    )
}

/// Same document, with the shared body wrapped in `wrapper_levels` levels
/// of `<section>`. Only the outermost carries `wrapper_style`; the rest are
/// completely unstyled.
fn doc(pin_style: &str, wrapper_levels: usize, wrapper_style: &str) -> String {
    let open = (0..wrapper_levels)
        .map(|i| {
            if i == 0 {
                format!("<section style=\"{wrapper_style}\">")
            } else {
                "<section>".to_string()
            }
        })
        .collect::<String>();
    let close = "</section>".repeat(wrapper_levels);
    format!("{HEAD}{open}{}{close}{TAIL}", inner(pin_style))
}

fn render(html: &str) -> Vec<u8> {
    Engine::builder()
        .page_size(PageSize {
            width: PAGE_W,
            height: PAGE_H,
        })
        .margin(Margin::uniform(MARGIN))
        .build()
        .render(html)
        .expect("render")
}

/// Text baselines in **pt from the page top**, ascending — so index 0 is the
/// topmost line on the page. krilla writes a Y-flipped text matrix, so the
/// `Tm` `f` operand is already top-down (cross-checked against
/// `mutool draw -F stext`, which reports the same numbers).
///
/// Returns `None` when qpdf is not installed, matching the other
/// content-stream tests in this crate.
fn baselines(html: &str) -> Option<Vec<f32>> {
    let mut ys = text_matrix_ys(&render(html))?;
    ys.sort_by(f32::total_cmp);
    Some(ys)
}

fn close(a: f32, b: f32) -> bool {
    (a - b).abs() < 0.01
}

/// The property the defect actually broke: an unstyled wrapper is not a
/// containing block for anything, so it must not change where a single
/// glyph lands. Asserted over the whole document, over four inset shapes
/// and three nesting depths, so it cannot pass by coincidence on one of
/// them.
#[test]
fn an_unstyled_wrapper_does_not_move_anything() {
    for pin_style in [
        "bottom: 0; left: 0; right: 0;",
        "bottom: 50pt; left: 0;",
        "top: 0; left: 0;",
        "top: 50%; left: 0;",
        "",
    ] {
        let Some(flat) = baselines(&doc(pin_style, 0, "")) else {
            eprintln!("qpdf not installed; skipping");
            return;
        };
        for levels in 1..=3 {
            let wrapped = baselines(&doc(pin_style, levels, "")).expect("qpdf");
            assert_eq!(
                flat, wrapped,
                "`{pin_style}` moved when wrapped in {levels} unstyled <section>(s)"
            );
        }
    }
}

/// The measured coordinate, checked against WeasyPrint 69 rather than
/// against fulgur's own previous output: a nested `bottom: 0` pins to the
/// page content area, exactly where the direct-child control lands.
#[test]
fn nested_absolute_bottom_zero_pins_to_the_page_area() {
    let Some(ys) = baselines(&doc("bottom: 0; left: 0; right: 0;", 1, "")) else {
        eprintln!("qpdf not installed; skipping");
        return;
    };
    assert_eq!(ys.len(), 3, "expected FLOW / MID / PIN, got {ys:?}");
    assert!(close(ys[0], FLOW), "FLOW moved: {ys:?}");
    assert!(close(ys[1], MID), "MID moved: {ys:?}");
    assert!(
        close(ys[2], PINNED_BOTTOM),
        "nested `bottom: 0` must pin to the page content area at {PINNED_BOTTOM}pt, got {ys:?}"
    );
}

/// A wrapper that *is* offset must not drag the absolute block with it
/// either: `top: 0` anchors to the page area, not to the wrapper's top.
/// WeasyPrint 69 puts PIN's baseline at the page-area top with and without
/// the 40pt-offset wrapper.
#[test]
fn an_offset_wrapper_does_not_move_a_top_anchored_absolute() {
    let Some(ys) = baselines(&doc("top: 0; left: 0;", 1, "margin-top: 40pt;")) else {
        eprintln!("qpdf not installed; skipping");
        return;
    };
    assert!(
        close(ys[0], FLOW),
        "`top: 0` must anchor to the page area ({FLOW}pt), not to the wrapper's \
         40pt-offset top ({}pt), got {ys:?}",
        FLOW + 40.0
    );
}

/// A `position: relative` ancestor *is* a containing block, so this shape
/// must keep resolving against the wrapper. Guards the fix against
/// over-reaching — WeasyPrint puts PIN on MID's baseline here.
#[test]
fn a_relative_wrapper_still_is_the_containing_block() {
    let Some(ys) = baselines(&doc(
        "bottom: 0; left: 0; right: 0;",
        1,
        "position: relative;",
    )) else {
        eprintln!("qpdf not installed; skipping");
        return;
    };
    assert_eq!(ys.len(), 3, "expected FLOW / MID / PIN, got {ys:?}");
    assert!(
        close(ys[1], MID) && close(ys[2], MID),
        "a relative wrapper is the containing block: its 124pt-tall box puts \
         `bottom: 0` on MID's baseline ({MID}pt), got {ys:?}"
    );
}

/// An absolute with no explicit inset keeps its static position, wrapper or
/// not — there is no containing block question to answer, and the in-flow
/// fragmenter already places it.
#[test]
fn an_absolute_without_insets_keeps_its_static_position() {
    let Some(ys) = baselines(&doc("", 1, "")) else {
        eprintln!("qpdf not installed; skipping");
        return;
    };
    assert_eq!(ys.len(), 3, "expected FLOW / MID / PIN, got {ys:?}");
    assert!(
        close(ys[2], STATIC_POS),
        "a no-inset absolute must stay one 12pt line below MID ({STATIC_POS}pt), got {ys:?}"
    );
}
