//! A `<thead>` must repeat at the top of every page its table spans.
//!
//! Both reference engines agree (WeasyPrint 69 and Chrome 151 put the header
//! on 7/7 pages of the 300-row fixture); fulgur drew it on page 1 only. Row
//! integrity was never the problem — 300/300 unique rows, zero duplicated,
//! before and after.
//!
//! The header text is matched by its own font size rather than by content,
//! since `inspect` returns raw glyph ids rather than text (lopdf does not
//! read krilla's `ToUnicode` CMap).

use fulgur::Engine;
use fulgur::inspect::inspect;

/// Build a table tall enough to span several pages. The header cells are set
/// at a distinctive size so their drawn runs can be told from body rows.
fn tall_table(rows: usize) -> String {
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
         tr {{ break-inside: avoid }}\
         </style></head><body><table>\
         <thead><tr><th>Code</th><th>Header</th><th>Amount</th></tr></thead>\
         <tbody>{body}</tbody></table></body></html>"
    )
}

/// `(pages, pages that carry at least one 13pt header run)`.
fn header_pages(rows: usize) -> (u32, usize) {
    let pdf = Engine::builder()
        .build()
        .render(&tall_table(rows))
        .expect("render must succeed");
    let dir = tempfile::tempdir().expect("create tempdir");
    let path = dir.path().join("out.pdf");
    std::fs::write(&path, &pdf).expect("write pdf");
    let r = inspect(&path).expect("inspect must succeed");

    let mut pages_with_header = std::collections::BTreeSet::new();
    for t in &r.text_items {
        if (t.font_size - 13.0).abs() < 0.01 {
            pages_with_header.insert(t.page);
        }
    }
    (r.pages, pages_with_header.len())
}

#[test]
fn a_thead_repeats_on_every_page_its_table_spans() {
    let (pages, with_header) = header_pages(300);
    assert!(
        pages > 1,
        "the fixture must span several pages, got {pages}"
    );
    assert_eq!(
        with_header, pages as usize,
        "the header belongs on every page the table spans, not only the first"
    );
}

/// A table that fits on one page must not gain a second header, and must not
/// be pushed onto a second page by a reservation that should not apply.
#[test]
fn a_single_page_table_is_unchanged() {
    let (pages, with_header) = header_pages(5);
    assert_eq!(pages, 1, "five rows should fit on one page");
    assert_eq!(with_header, 1, "exactly one header on a single-page table");
}

/// Every run drawn at the body font size, as `(page, y)`.
///
/// `inspect` returns raw glyph ids rather than text (lopdf does not read
/// krilla's `ToUnicode` CMap), and its `width` is a crude
/// `chars x font_size` estimate — so runs are identified by font size and
/// compared by position only. `y` is PDF user space: it grows *upward*,
/// so a larger `y` is higher on the page.
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

/// Row integrity: the fragmenter must neither drop nor duplicate a row
/// while splitting the table. Three cells per row at 9pt, 300 rows, so
/// exactly 900 body runs — any loss or duplication moves this count.
///
/// This is the invariant a careless reservation change breaks, and it was
/// already perfect before header repetition existed (300/300 unique rows,
/// zero duplicated, measured against WeasyPrint 69).
#[test]
fn every_body_row_is_drawn_exactly_once() {
    let pdf = Engine::builder()
        .build()
        .render(&tall_table(300))
        .expect("render must succeed");
    let body = runs_at_size(&pdf, 9.0);
    assert_eq!(
        body.len(),
        900,
        "300 rows x 3 cells must be drawn exactly once each"
    );
}

/// The header must *reserve* its space, not merely be repeated.
///
/// Emitting header fragments without reservation is the tempting cheap
/// route (it is how `append_position_fixed_fragments` handles
/// `position: fixed`), but `position: fixed` is out of flow and displaces
/// nothing. A header drawn without reservation lands on top of the first
/// row of every continuation page — worse than omitting it. This asserts
/// the separation directly: on every page, every header run sits strictly
/// above every body run.
#[test]
fn the_repeated_header_never_overlaps_a_body_row() {
    let pdf = Engine::builder()
        .build()
        .render(&tall_table(300))
        .expect("render must succeed");
    let headers = runs_at_size(&pdf, 13.0);
    let bodies = runs_at_size(&pdf, 9.0);

    let pages: std::collections::BTreeSet<u32> = headers.iter().map(|&(p, _)| p).collect();
    assert!(pages.len() > 1, "fixture must span several pages");

    for page in pages {
        let header_low = headers
            .iter()
            .filter(|&&(p, _)| p == page)
            .map(|&(_, y)| y)
            .fold(f32::INFINITY, f32::min);
        let body_high = bodies
            .iter()
            .filter(|&&(p, _)| p == page)
            .map(|&(_, y)| y)
            .fold(f32::NEG_INFINITY, f32::max);
        assert!(
            header_low > body_high,
            "page {page}: header bottom ({header_low}) must sit above the \
             topmost body row ({body_high}) — a reservation was not applied"
        );
    }
}

/// A header too tall to leave room for a row is dropped for that page
/// rather than starving it.
///
/// WeasyPrint's rule (`layout/table.py::all_groups_layout`): "If no row can
/// be rendered because of the header and the footer, the header and/or the
/// footer are not rendered." Deciding this per page is what guarantees the
/// fragmenter still makes progress — the row content must survive intact
/// even when repetition is abandoned.
#[test]
fn a_header_too_tall_to_repeat_is_dropped_without_losing_rows() {
    let html = tall_table(60).replace("font-size: 13pt", "font-size: 700pt");
    let pdf = Engine::builder()
        .build()
        .render(&html)
        .expect("render must succeed");
    let body = runs_at_size(&pdf, 9.0);
    assert_eq!(
        body.len(),
        180,
        "60 rows x 3 cells must survive even when the header cannot repeat"
    );
}
