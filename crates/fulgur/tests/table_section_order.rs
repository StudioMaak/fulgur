//! Where a table's sections are *written* must not change where they are
//! *drawn*.
//!
//! CSS 2.1 §17.5.1 orders a table's boxes header group → row groups →
//! footer group, whatever the source order. HTML4 in fact *required*
//! `<tfoot>` to be written before `<tbody>`, so real documents carry the
//! shape. Blitz builds the table's cell grid by walking the table's DOM
//! children in order (`blitz-dom` `layout/table.rs::collect_table_cells`
//! gives `TableHeaderGroup` / `TableFooterGroup` the same treatment as
//! `TableRowGroup`), so fulgur used to render a `<tfoot>` written first at
//! the *top* of the table.
//!
//! Both reference engines agree on every case asserted here, and — the
//! decisive property — WeasyPrint 69's output for the two orderings is
//! byte-for-byte identical. Measured on 40 rows, A4/20mm, `yMin` top-down:
//!
//! | fixture              | WeasyPrint 69 head / rows / foot |
//! |----------------------|----------------------------------|
//! | `<tfoot>` before     | 64.7 / 79-643 / 657.8            |
//! | `<tfoot>` after      | 64.7 / 79-643 / 657.8            |
//!
//! Runs are matched by font size rather than content, since `inspect`
//! returns raw glyph ids (lopdf does not read krilla's `ToUnicode` CMap),
//! and its `width` is a crude `chars x font_size` estimate — never assert
//! on it. `y` is PDF user space and grows *upward*, so a larger `y` is
//! higher on the page.

use fulgur::Engine;
use fulgur::inspect::inspect;

const HEAD_PT: f32 = 13.0;
const FOOT_PT: f32 = 11.0;
const BODY_PT: f32 = 9.0;

const HEAD: &str = "<thead><tr><th>Code</th><th>Header</th></tr></thead>";
const FOOT: &str = "<tfoot><tr><td>Total</td><td>Footer</td></tr></tfoot>";

fn rows(n: usize) -> String {
    (0..n)
        .map(|i| format!("<tr><td>R{i:04}</td><td>Item {i}</td></tr>"))
        .collect()
}

/// A table whose three bands are drawn at three distinct font sizes, so each
/// is identifiable in the output without recovering any text.
fn table(sections: &str) -> String {
    format!(
        "<!doctype html><html><head><style>\
         @page {{ size: A4; margin: 20mm }}\
         table {{ width: 100%; border-collapse: collapse; font-size: {BODY_PT}pt }}\
         th, td {{ border-bottom: .5pt solid #999; padding: 2pt 3pt }}\
         th {{ font-size: {HEAD_PT}pt }}\
         tfoot td {{ font-size: {FOOT_PT}pt }}\
         tr {{ break-inside: avoid }}\
         </style></head><body><table>{sections}</table></body></html>"
    )
}

fn render(html: &str) -> Vec<u8> {
    Engine::builder()
        .build()
        .render(html)
        .expect("render must succeed")
}

/// Every drawn run as `(page, font_size, x, y)`, rounded to 0.01pt so the
/// two orderings can be compared for exact equality.
fn runs(pdf: &[u8]) -> Vec<(u32, i64, i64, i64)> {
    let dir = tempfile::tempdir().expect("create tempdir");
    let path = dir.path().join("out.pdf");
    std::fs::write(&path, pdf).expect("write pdf");
    let mut out: Vec<(u32, i64, i64, i64)> = inspect(&path)
        .expect("inspect must succeed")
        .text_items
        .iter()
        .map(|t| {
            (
                t.page,
                (t.font_size * 100.0).round() as i64,
                (t.x * 100.0).round() as i64,
                (t.y * 100.0).round() as i64,
            )
        })
        .collect();
    out.sort_unstable();
    out
}

/// `(page, y)` of every run drawn at `size`.
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

/// The property, stated as WeasyPrint satisfies it: the two source
/// orderings must render identically — not merely "the footer ends up low
/// enough". This is the assertion the whole change exists to make true.
#[test]
fn source_order_of_the_sections_does_not_change_the_output() {
    let before = render(&table(&format!("{HEAD}{FOOT}<tbody>{}</tbody>", rows(40))));
    let after = render(&table(&format!("{HEAD}<tbody>{}</tbody>{FOOT}", rows(40))));
    assert_eq!(
        runs(&before),
        runs(&after),
        "`<tfoot>` before `<tbody>` must draw exactly what `<tfoot>` after `<tbody>` draws"
    );
}

/// Same property for a `<thead>` written last, which the HTML parser also
/// accepts. Both references place it at the top either way.
#[test]
fn a_thead_written_after_tbody_still_renders_at_the_top() {
    let last = render(&table(&format!("<tbody>{}</tbody>{HEAD}", rows(40))));
    let first = render(&table(&format!("{HEAD}<tbody>{}</tbody>", rows(40))));
    assert_eq!(
        runs(&last),
        runs(&first),
        "`<thead>` written after `<tbody>` must draw exactly what `<thead>` written first draws"
    );
}

/// The absolute placement behind the property, so a regression that moved
/// *both* orderings the same wrong way could not pass. 40 rows fit one page.
#[test]
fn a_tfoot_written_before_tbody_is_drawn_below_the_last_row() {
    let pdf = render(&table(&format!("{HEAD}{FOOT}<tbody>{}</tbody>", rows(40))));
    let head = runs_at_size(&pdf, HEAD_PT);
    let foot = runs_at_size(&pdf, FOOT_PT);
    let body = runs_at_size(&pdf, BODY_PT);
    assert_eq!(pages_of(&head).len(), 1, "40 rows fit a single page");
    assert!(!foot.is_empty(), "the footer must be drawn");

    // y grows upward, so "below" is a smaller y.
    let lowest_body = body.iter().map(|&(_, y)| y).fold(f32::INFINITY, f32::min);
    let highest_head = head
        .iter()
        .map(|&(_, y)| y)
        .fold(f32::NEG_INFINITY, f32::max);
    for &(_, y) in &foot {
        assert!(
            y < lowest_body,
            "footer at y={y} must sit below the lowest body row at y={lowest_body}"
        );
        assert!(
            y < highest_head,
            "footer at y={y} must sit below the header at y={highest_head}"
        );
    }
}

/// A `<thead>` written last must reserve room at the *top*, above the first
/// row, rather than after it.
#[test]
fn a_thead_written_after_tbody_is_drawn_above_the_first_row() {
    let pdf = render(&table(&format!("<tbody>{}</tbody>{HEAD}", rows(40))));
    let head = runs_at_size(&pdf, HEAD_PT);
    let body = runs_at_size(&pdf, BODY_PT);
    let highest_body = body
        .iter()
        .map(|&(_, y)| y)
        .fold(f32::NEG_INFINITY, f32::max);
    assert!(!head.is_empty(), "the header must be drawn");
    for &(_, y) in &head {
        assert!(
            y > highest_body,
            "header at y={y} must sit above the highest body row at y={highest_body}"
        );
    }
}

/// Once the footer is genuinely the table's bottom band, defect 2's footer
/// repetition applies to it like any other — `table_section_band` no longer
/// has a misplaced band to bail on. WeasyPrint repeats the footer on every
/// page for *both* orderings, its two outputs being identical.
#[test]
fn a_tfoot_written_before_tbody_repeats_on_every_page_its_table_spans() {
    let pdf = render(&table(&format!("{HEAD}{FOOT}<tbody>{}</tbody>", rows(300))));
    let head = runs_at_size(&pdf, HEAD_PT);
    let foot = runs_at_size(&pdf, FOOT_PT);
    let body = runs_at_size(&pdf, BODY_PT);
    let body_pages = pages_of(&body);
    assert!(body_pages.len() > 1, "300 rows must span several pages");
    assert_eq!(
        pages_of(&foot),
        body_pages,
        "the footer must be drawn on every page the table spans"
    );
    assert_eq!(
        pages_of(&head),
        body_pages,
        "the header must be drawn on every page the table spans"
    );
}

/// Only a *footer group* moves. `display: table-row-group` on a `<tfoot>`
/// makes it an ordinary row group, and both references then leave it in
/// source order — WeasyPrint draws it at yMin 79.2, above the first row at
/// 93.6, and Chrome 151 agrees. Keying the reorder on the computed display
/// rather than on the tag name is what keeps this correct.
#[test]
fn a_tfoot_demoted_to_a_row_group_keeps_its_source_position() {
    let sections = format!(
        "{HEAD}<tfoot style=\"display:table-row-group\"><tr>\
         <td>Total</td><td>Footer</td></tr></tfoot><tbody>{}</tbody>",
        rows(10)
    );
    let pdf = render(&table(&sections));
    let foot = runs_at_size(&pdf, FOOT_PT);
    let body = runs_at_size(&pdf, BODY_PT);
    let highest_body = body
        .iter()
        .map(|&(_, y)| y)
        .fold(f32::NEG_INFINITY, f32::max);
    assert!(!foot.is_empty(), "the demoted footer must still be drawn");
    for &(_, y) in &foot {
        assert!(
            y > highest_body,
            "a `display:table-row-group` `<tfoot>` keeps its source position at y={y}, \
             above the first body row at y={highest_body}"
        );
    }
}

/// With several header / footer groups, CSS 2.1 §17.5.1 promotes only the
/// *first* of each; the rest stay put as ordinary row groups. Measured on
/// `<tfoot A><thead A><tbody><tfoot B><thead B>`, both references draw
/// `HEAD A → rows → FOOT B → HEAD B → FOOT A`, top to bottom.
#[test]
fn only_the_first_header_and_footer_group_are_promoted() {
    let html = format!(
        "<!doctype html><html><head><style>\
         @page {{ size: A4; margin: 20mm }}\
         table {{ width: 100%; border-collapse: collapse; font-size: {BODY_PT}pt }}\
         th, td {{ border-bottom: .5pt solid #999; padding: 2pt 3pt }}\
         .fa td {{ font-size: 11pt }} .ha th {{ font-size: 13pt }}\
         .fb td {{ font-size: 15pt }} .hb th {{ font-size: 17pt }}\
         </style></head><body><table>\
         <tfoot class=\"fa\"><tr><td>FA</td></tr></tfoot>\
         <thead class=\"ha\"><tr><th>HA</th></tr></thead>\
         <tbody><tr><td>R0</td></tr><tr><td>R1</td></tr></tbody>\
         <tfoot class=\"fb\"><tr><td>FB</td></tr></tfoot>\
         <thead class=\"hb\"><tr><th>HB</th></tr></thead>\
         </table></body></html>"
    );
    let pdf = render(&html);
    let y_of = |size: f32| {
        let r = runs_at_size(&pdf, size);
        assert!(!r.is_empty(), "a run at {size}pt must be drawn");
        r[0].1
    };
    let (ha, hb) = (y_of(13.0), y_of(17.0));
    let (fa, fb) = (y_of(11.0), y_of(15.0));
    let rows_y = runs_at_size(&pdf, BODY_PT);
    let lowest_row = rows_y.iter().map(|&(_, y)| y).fold(f32::INFINITY, f32::min);

    // y grows upward: top to bottom is descending y.
    assert!(
        ha > lowest_row,
        "the first `<thead>` is promoted to the top"
    );
    assert!(
        lowest_row > fb,
        "the second `<tfoot>` stays below the rows, in source order"
    );
    assert!(
        fb > hb,
        "the second `<thead>` follows the second `<tfoot>`, in source order"
    );
    assert!(
        hb > fa,
        "the first `<tfoot>` is demoted to the very bottom of the table"
    );
}
