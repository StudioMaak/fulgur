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
#[ignore = "unimplemented: <thead> repetition needs space reservation in \
            fragment_block_subtree; see paperworx-repros/README.md item 2"]
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
