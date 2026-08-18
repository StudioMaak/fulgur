//! A `margin-top` adjoining a **forced** break is retained, not truncated.
//!
//! css-break-3 §5.4: "When an unforced break occurs before or after a
//! block-level box, any margins adjoining the break are truncated to zero.
//! When a forced break occurs there, adjoining margins *before* the break are
//! truncated, but margins *after* the break are preserved."
//!
//! fulgur truncated both sides at a forced break — `fragment_pagination_root`
//! reset `cursor_y` to 0 and `fragment_block_subtree` rebased
//! `page_taffy_origin` onto the child's own top, in both cases discarding the
//! whole inter-child gap. Written up as defect 11 in
//! `paperworx-repros/README.md`; the committed repro is
//! `paperworx-repros/11-margin-after-forced-break.html`.
//!
//! Every assertion here is a **delta** between two renders of the same
//! document that differ only in the margin's length, so no glyph metric,
//! ascent or line-height enters the arithmetic — the font cancels out
//! exactly. Measured against WeasyPrint 69, which moves the page-2 baseline
//! from 23.52mm to 31.99mm when `margin-top: 24pt` is added (8.47mm = 24pt).

use fulgur::Engine;
use fulgur::inspect::inspect;

const MARGIN_PT: f32 = 24.0;
/// Every fixture below pins `@page { size: A4 }`.
const A4_HEIGHT_PT: f32 = 841.89;

/// Distance in PDF pt from the top of `page` (1-based) down to the topmost
/// text on it. `inspect` reports PDF-native user space — bottom-left origin,
/// y growing *up* — so the topmost item is the one with the largest `y`.
fn text_top_on_page(html: &str, page: u32) -> f32 {
    let pdf = Engine::builder()
        .build()
        .render(html)
        .expect("render must succeed");
    let dir = tempfile::tempdir().expect("create tempdir");
    let path = dir.path().join("out.pdf");
    std::fs::write(&path, &pdf).expect("write pdf");
    let result = inspect(&path).expect("inspect must succeed");
    assert!(
        result.pages >= page,
        "expected at least {page} pages, got {}",
        result.pages
    );
    let highest = result
        .text_items
        .iter()
        .filter(|t| t.page == page)
        .map(|t| t.y)
        .fold(f32::NEG_INFINITY, f32::max);
    assert!(
        highest.is_finite(),
        "page {page} carries no text; the fixture did not paginate as intended"
    );
    A4_HEIGHT_PT - highest
}

/// How far *down* the top of page 2 moves when the fixture's `MARGIN`
/// placeholder goes from `0` to `24pt`. Retained → 24pt; truncated → 0.
fn page_two_shift(template: &str) -> f32 {
    let without = text_top_on_page(&template.replace("MARGIN", "0"), 2);
    let with = text_top_on_page(&template.replace("MARGIN", "24pt"), 2);
    with - without
}

fn doc(style: &str, body: &str) -> String {
    format!(
        "<!doctype html><html><head><meta charset=utf-8><style>\
         @page {{ size: A4; margin: 20mm }}\
         body {{ margin: 0 }}\
         p {{ margin: 0; font-size: 10pt; line-height: 12pt }}\
         {style}</style></head><body>{body}</body></html>"
    )
}

/// The committed repro's shape: the margin sits on a *child* of the box that
/// carries `break-before`, so it collapses up through it. The retained value
/// is the collapsed-through margin, which is what Taffy reports as the
/// breaking box's own `layout.margin.top`.
#[test]
fn a_margin_collapsed_up_into_the_breaking_box_is_retained() {
    let html = doc(
        ".brk { break-before: page } .gap { margin-top: MARGIN }",
        "<p>PAGE1</p><section class=brk><section class=gap><p>PAGE2</p></section></section>",
    );
    let shift = page_two_shift(&html);
    assert!(
        (shift - MARGIN_PT).abs() < 0.5,
        "a margin adjoining a forced break is preserved (css-break-3 §5.4): \
         page 2 must move down by {MARGIN_PT}pt, moved by {shift}pt"
    );
}

/// The same thing stated directly: the `margin-top` is on the very box that
/// carries `break-before: page`.
#[test]
fn a_margin_on_the_breaking_box_itself_is_retained() {
    let html = doc(
        ".brk { break-before: page; margin-top: MARGIN }",
        "<p>PAGE1</p><div class=brk><p>PAGE2</p></div>",
    );
    let shift = page_two_shift(&html);
    assert!(
        (shift - MARGIN_PT).abs() < 0.5,
        "page 2 must move down by {MARGIN_PT}pt, moved by {shift}pt"
    );
}

/// The same again one level down, so the break is taken by
/// `fragment_block_subtree` rather than `fragment_pagination_root`. The
/// wrapper's `border-top` stops its own margins collapsing with the
/// children's, which is what keeps the break body-indirect.
#[test]
fn a_nested_forced_break_retains_the_margin() {
    let html = doc(
        ".outer { border-top: 1pt solid transparent } \
         .brk { break-before: page; margin-top: MARGIN }",
        "<div class=outer><p>PAGE1</p><div class=brk><p>PAGE2</p></div></div>",
    );
    let shift = page_two_shift(&html);
    assert!(
        (shift - MARGIN_PT).abs() < 0.5,
        "a forced break inside a block subtree preserves the margin after it \
         too: page 2 must move down by {MARGIN_PT}pt, moved by {shift}pt"
    );
}

/// A page-name change forces a break of its own (CSS Page 3 §5.3), and the
/// fragmenter already treats it identically to an authored `break-before:
/// page`. §5.4 therefore preserves the margin after it too, and WeasyPrint 69
/// does: the page-2 baseline moves 23.52mm → 31.99mm exactly as for an
/// authored break.
#[test]
fn a_page_name_change_retains_the_margin_too() {
    let html = doc(
        "@page named { size: A4; margin: 20mm } \
         .named { page: named; margin-top: MARGIN }",
        "<p>PAGE1</p><div class=named><p>PAGE2</p></div>",
    );
    let shift = page_two_shift(&html);
    assert!(
        (shift - MARGIN_PT).abs() < 0.5,
        "a page-name change is a forced break: page 2 must move down by \
         {MARGIN_PT}pt, moved by {shift}pt"
    );
}

/// Control — the *other* half of §5.4. Here the gap comes entirely from the
/// **previous** sibling's `margin-bottom`, which adjoins the break on the
/// before side and must still be truncated. WeasyPrint 69 leaves page 2's
/// baseline at 23.52mm whether or not that margin is present.
///
/// This is the case that makes "retain the whole inter-child gap" wrong, and
/// the reason the retained value has to be the breaking box's own collapsed
/// top margin rather than the gap.
#[test]
fn a_previous_siblings_bottom_margin_is_still_truncated() {
    let html = doc(
        ".pre { margin-bottom: MARGIN } .brk { break-before: page }",
        "<p class=pre>PAGE1</p><div class=brk><p>PAGE2</p></div>",
    );
    let shift = page_two_shift(&html);
    assert!(
        shift.abs() < 0.5,
        "a margin *before* a forced break is truncated (css-break-3 §5.4): \
         page 2 must not move, moved by {shift}pt"
    );
}

/// Control — an **unforced** break still truncates. This is the behaviour
/// §5.4 keeps, and the one most easily lost while restoring the forced case:
/// the block below is pushed to page 2 by overflow alone, with no break
/// property anywhere in the document.
#[test]
fn a_margin_at_an_unforced_break_is_still_truncated() {
    let html = doc(
        ".tall { height: 200mm } .gap { margin-top: MARGIN; height: 80mm }",
        "<div class=tall><p>PAGE1</p></div><div class=gap><p>PAGE2</p></div>",
    );
    let shift = page_two_shift(&html);
    assert!(
        shift.abs() < 0.5,
        "a margin adjoining an unforced break is truncated to zero: \
         page 2 must not move, moved by {shift}pt"
    );
}

/// Control — `break-inside: avoid` relocating a box to the next page is an
/// **unforced** break (css-break-3 §4.3: a forced break is one indicated by a
/// forced value of `break-before` / `break-after`), so its margin truncates
/// as well. Restoring the forced case must not leak into this path.
#[test]
fn a_margin_before_a_break_inside_avoid_relocation_is_still_truncated() {
    let html = doc(
        ".tall { height: 200mm } \
         .keep { break-inside: avoid; margin-top: MARGIN; height: 80mm }",
        "<div class=tall><p>PAGE1</p></div><div class=keep><p>PAGE2</p></div>",
    );
    let shift = page_two_shift(&html);
    assert!(
        shift.abs() < 0.5,
        "`break-inside: avoid` is not a forced break: page 2 must not move, \
         moved by {shift}pt"
    );
}
