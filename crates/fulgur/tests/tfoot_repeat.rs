//! A `<tfoot>` must repeat at the bottom of every page its table spans.
//!
//! WeasyPrint 69 puts the footer on 7/7 pages of the 300-row fixture and
//! places it immediately after the last body row on each page, not flush to
//! the page bottom (`layout/table.py::all_groups_layout` translates the
//! footer to `end_position_y`). fulgur drew it on the final page only.
//!
//! Footer reservation is the mirror of the header's, not a copy: a repeated
//! header offsets where each page's strip *starts*, a repeated footer
//! shrinks where it *ends*.
//!
//! Scope: these fixtures put `<tfoot>` after `<tbody>`. fulgur lays table
//! sections out in source order, so a `<tfoot>` written *before* `<tbody>`
//! renders at the top of the table — a separate, single-page layout defect
//! tracked in `paperworx-repros/README.md`, not addressed here. The band
//! helper bails on that shape rather than repeating a misplaced footer.
//!
//! Runs are matched by font size rather than content, since `inspect`
//! returns raw glyph ids (lopdf does not read krilla's `ToUnicode` CMap),
//! and its `width` is a crude `chars x font_size` estimate.

use fulgur::Engine;
use fulgur::inspect::inspect;

/// Header at 13pt, footer at 11pt, body at 9pt, so all three are
/// distinguishable in the drawn output by size alone.
fn tall_table_with_footer(rows: usize) -> String {
    let body: String = (0..rows)
        .map(|i| {
            format!(
                "<tr><td>R{i:04}</td><td>Item {i}</td><td>{:.2}</td></tr>",
                i as f32 * 7.5
            )
        })
        .collect();
    format!(
        "<!doctype html><html><head><style>\
         @page {{ size: A4; margin: 20mm }}\
         table {{ width: 100%; border-collapse: collapse; font-size: 9pt }}\
         th, td {{ border-bottom: .5pt solid #999; padding: 2pt 3pt }}\
         th {{ font-size: 13pt }}\
         tfoot td {{ font-size: 11pt }}\
         tr {{ break-inside: avoid }}\
         </style></head><body><table>\
         <thead><tr><th>Code</th><th>Header</th><th>Amount</th></tr></thead>\
         <tbody>{body}</tbody>\
         <tfoot><tr><td>Total</td><td>Footer</td><td>0.00</td></tr></tfoot>\
         </table></body></html>"
    )
}

fn render(rows: usize) -> Vec<u8> {
    Engine::builder()
        .build()
        .render(&tall_table_with_footer(rows))
        .expect("render must succeed")
}

/// Every run drawn at `size`, as `(page, y)`. `y` is PDF user space and
/// grows *upward*, so a larger `y` is higher on the page.
fn runs_at_size(pdf: &[u8], size: f32) -> Vec<(u32, f32)> {
    let dir = tempfile::tempdir().expect("create tempdir");
    let path = dir.path().join("out.pdf");
    std::fs::write(&path, pdf).expect("write pdf");
    inspect(&path)
        .expect("inspect must succeed")
        .text_items
        .iter()
        .filter(|t| (t.font_size - size).abs() < 0.01)
        .map(|t| (t.page, t.y))
        .collect()
}

fn pages_of(runs: &[(u32, f32)]) -> std::collections::BTreeSet<u32> {
    runs.iter().map(|&(p, _)| p).collect()
}

#[test]
fn a_tfoot_repeats_on_every_page_its_table_spans() {
    let pdf = render(300);
    let body_pages = pages_of(&runs_at_size(&pdf, 9.0));
    let foot_pages = pages_of(&runs_at_size(&pdf, 11.0));
    assert!(
        body_pages.len() > 1,
        "the fixture must span several pages, got {}",
        body_pages.len()
    );
    assert_eq!(
        foot_pages, body_pages,
        "the footer belongs on every page the table spans, not only the last"
    );
}

/// Row integrity across a footer reservation. Reserving space at the bottom
/// of every page is exactly the kind of change that silently drops the last
/// row of a page, so pin the count: 300 rows x 3 cells, each drawn once.
#[test]
fn every_body_row_is_drawn_exactly_once_with_a_footer() {
    let pdf = render(300);
    assert_eq!(
        runs_at_size(&pdf, 9.0).len(),
        900,
        "300 rows x 3 cells must be drawn exactly once each"
    );
}

/// The footer must reserve its space, not merely be repeated: without a
/// reservation the body fills the full strip and the repeated footer lands
/// on top of the last row.
#[test]
fn the_repeated_footer_never_overlaps_the_last_row() {
    let pdf = render(300);
    let footers = runs_at_size(&pdf, 11.0);
    let bodies = runs_at_size(&pdf, 9.0);

    for page in pages_of(&footers) {
        let footer_high = footers
            .iter()
            .filter(|&&(p, _)| p == page)
            .map(|&(_, y)| y)
            .fold(f32::NEG_INFINITY, f32::max);
        let body_low = bodies
            .iter()
            .filter(|&&(p, _)| p == page)
            .map(|&(_, y)| y)
            .fold(f32::INFINITY, f32::min);
        assert!(
            footer_high < body_low,
            "page {page}: footer top ({footer_high}) must sit below the \
             lowest body row ({body_low}) — a reservation was not applied"
        );
    }
}

/// A table that fits on one page must not gain a second footer, nor be
/// pushed onto a second page by a reservation that should not apply.
#[test]
fn a_single_page_table_with_a_footer_is_unchanged() {
    let pdf = render(5);
    let dir = tempfile::tempdir().expect("create tempdir");
    let path = dir.path().join("out.pdf");
    std::fs::write(&path, &pdf).expect("write pdf");
    assert_eq!(inspect(&path).expect("inspect").pages, 1, "five rows fit");
    assert_eq!(
        runs_at_size(&pdf, 11.0).len(),
        3,
        "exactly one footer row (3 cells) on a single-page table"
    );
}
