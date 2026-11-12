//! Page 0's usable height is short by the body's own offset.
//!
//! `render_v2` shifts page 0's fragments down by `body_offset_pt.1` — the
//! body element's `location.y`, which absorbs the collapsed top margin of the
//! first in-flow child — and applies it to page 0 only ("continuation pages
//! are already page-content-area-relative after the fragmenter resets
//! cursor_y", `render.rs`). The fragmenter, however, compared body-relative
//! cursors against the *full* page height, so page 0 was over-filled by
//! exactly that offset and the last block on it overflowed the page bottom.
//!
//! Found by `scripts/refdiff` as a page-count divergence from WeasyPrint
//! (`break-inside-avoid.html`: fulgur 2 pages, WeasyPrint 3), and written up
//! as defect 7 in `paperworx-repros/README.md`. It is not a `break-inside`
//! defect — the same divergence reproduces with `break-inside: auto` and with
//! empty blocks.
//!
//! The fixture is deliberately font-independent: every height is an explicit
//! `mm`, so no glyph metric enters the arithmetic.

use fulgur::Engine;
use fulgur::inspect::inspect;

/// A4 with 20mm margins leaves a 257mm content band. A 30mm collapsed top
/// margin puts body's origin 30mm down, so page 0 really holds 227mm.
///
/// Blocks of 115mm therefore stack to 115mm and 230mm: the second block's
/// bottom falls inside the `(227mm, 257mm]` window that the two capacity
/// models disagree about. Reading page height as 257mm on page 0 fits two
/// blocks there and paginates to 2 pages; reading it as 227mm fits one and
/// paginates to 3 — which is what WeasyPrint 69 does.
fn fixture() -> String {
    let blocks: String = (0..4).map(|i| format!("<div class=b>B{i}</div>")).collect();
    format!(
        "<!doctype html><html><head><style>\
         @page {{ size: A4; margin: 20mm }}\
         body {{ margin: 0 }}\
         .lead {{ height: 0; margin-top: 30mm }}\
         .b {{ height: 115mm }}\
         </style></head><body><div class=lead></div>{blocks}</body></html>"
    )
}

fn pages(html: &str) -> u32 {
    let pdf = Engine::builder()
        .build()
        .render(html)
        .expect("render must succeed");
    let dir = tempfile::tempdir().expect("create tempdir");
    let path = dir.path().join("out.pdf");
    std::fs::write(&path, &pdf).expect("write pdf");
    inspect(&path).expect("inspect must succeed").pages
}

#[test]
fn page_zero_capacity_excludes_the_body_offset() {
    assert_eq!(
        pages(&fixture()),
        3,
        "page 0 holds 227mm, not 257mm: one 115mm block, not two — \
         over-filling it by the body offset overflows the page bottom"
    );
}

/// The control: with no leading margin the body offset is zero, so page 0 has
/// the full 257mm and two 115mm blocks legitimately fit on it. This guards
/// against "fixing" the capacity by shrinking every page unconditionally,
/// which would under-fill documents that have no offset at all.
#[test]
fn a_document_without_a_body_offset_keeps_the_full_first_page() {
    let html = fixture().replace("margin-top: 30mm", "margin-top: 0");
    assert_eq!(
        pages(&html),
        2,
        "with no body offset, page 0 holds two 115mm blocks"
    );
}
