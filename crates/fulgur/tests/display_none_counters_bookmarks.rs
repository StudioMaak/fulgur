//! An element with `display: none` generates no box, so it neither changes
//! counters nor gets a PDF outline entry, and neither do its descendants
//! (CSS Lists 3 §4). Running elements are `display: none` too after GCPM
//! rewrites them, but they still count: Prince 17 honours `counter-*` and
//! bookmarks on a running element and inside one.
//!
//! Reference: Prince 17 and Chrome agree on every non-running case below;
//! the running-element cases are Prince only (Chrome has no
//! `position: running()`).

use fulgur::Engine;

const STYLE: &str = r#"
@page { size: 400px 400px; margin: 50px; }
html, body { margin: 0; }
body { font-family: sans-serif; font-size: 15px; line-height: 20px; counter-reset: x; }
p { margin: 0; }
.show::after { content: "CTR" counter(x); }
.hdr-run { position: running(hdr); }
@page { @top-center { content: element(hdr); } }
"#;

/// Where the marker sits. The last wrap splits its container so the marker
/// lands on page 2.
const WRAPS: &[(&str, &str, &str)] = &[
    ("body", "", ""),
    ("section", "<section>", "</section>"),
    ("two deep", "<section><div>", "</div></section>"),
    (
        "split container",
        concat!(
            "<section>",
            r#"<div style="height:100px">FILL1</div>"#,
            r#"<div style="height:100px">FILL2</div>"#,
            r#"<div style="height:100px">FILL3</div>"#,
            r#"<div style="height:100px">FILL4</div>"#
        ),
        "</section>",
    ),
];

fn html(css: &str, wrap: (&str, &str), marker: &str) -> String {
    let (open, close) = wrap;
    format!(
        "<!DOCTYPE html><html><head><style>{STYLE}{css}</style></head><body>\
         {open}{marker}<p class=\"show\">BODYTEXT</p>{close}</body></html>"
    )
}

/// The words of the whole document via `pdftotext`. `None` (the test skips)
/// when poppler's pdftotext is not available; required on Linux CI, where
/// ci.yml installs poppler-utils.
fn words(pdf: &[u8]) -> Option<Vec<String>> {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("out.pdf");
    std::fs::write(&path, pdf).unwrap();
    let required = cfg!(target_os = "linux") && std::env::var_os("CI").is_some();
    let out = match std::process::Command::new("pdftotext")
        .arg(&path)
        .arg("-")
        .output()
    {
        Ok(out) if out.status.success() => out,
        Ok(out) if required => panic!("pdftotext failed on Linux CI: {out:?}"),
        Err(e) if required => panic!("pdftotext is required on Linux CI: {e}"),
        _ => return None,
    };
    Some(
        String::from_utf8(out.stdout)
            .unwrap()
            .split_whitespace()
            .map(str::to_string)
            .collect(),
    )
}

/// Render `marker` in every wrap and assert the body's `counter(x)` reads
/// `expected`.
fn assert_counter(css: &str, marker: &str, expected: i32) {
    let want = format!("CTR{expected}");
    let mut failures = Vec::new();
    for &(name, open, close) in WRAPS {
        let pdf = Engine::builder()
            .build()
            .render(&html(css, (open, close), marker))
            .expect("render");
        let Some(words) = words(&pdf) else {
            return;
        };
        // `::after` is inline, so the value is glued to BODYTEXT.
        let found: Vec<&str> = words
            .iter()
            .filter_map(|w| w.find("CTR").map(|i| &w[i..]))
            .collect();
        if found != [want.as_str()] {
            failures.push(format!("{name}: expected [{want}], found {found:?}"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn counter_increment_on_display_none_is_ignored() {
    assert_counter(
        ".c { counter-increment: x 5; display: none; }",
        r#"<div class="c"></div>"#,
        0,
    );
}

#[test]
fn counter_increment_inside_display_none_is_ignored() {
    assert_counter(
        ".c { counter-increment: x 5; }",
        r#"<div style="display:none"><div class="c"></div></div>"#,
        0,
    );
}

#[test]
fn counter_reset_on_display_none_is_ignored() {
    assert_counter(
        ".a { counter-increment: x 5; } .r { counter-reset: x 100; display: none; }",
        r#"<div class="a"></div><div class="r"></div>"#,
        5,
    );
}

#[test]
fn counter_set_on_display_none_is_ignored() {
    assert_counter(
        ".a { counter-increment: x 5; } .r { counter-set: x 100; display: none; }",
        r#"<div class="a"></div><div class="r"></div>"#,
        5,
    );
}

/// Control: the same increment on a box.
#[test]
fn counter_increment_on_box_counts() {
    assert_counter(
        ".c { counter-increment: x 5; }",
        r#"<div class="c"></div>"#,
        5,
    );
}

#[test]
fn counter_increment_on_running_element_counts() {
    assert_counter(
        ".hdr-run { counter-increment: x 5; }",
        r#"<div class="hdr-run">HDRTEXT</div>"#,
        5,
    );
}

#[test]
fn counter_increment_inside_running_element_counts() {
    assert_counter(
        ".c { counter-increment: x 5; }",
        r#"<div class="hdr-run"><div class="c">HDRTEXT</div></div>"#,
        5,
    );
}

/// A running element whose `display: none` lost the cascade is a box.
#[test]
fn counter_increment_on_running_element_kept_as_box_counts() {
    assert_counter(
        ".hdr-run { counter-increment: x 5; }",
        r#"<div class="hdr-run" style="display:block">HDRTEXT</div>"#,
        5,
    );
}

/// Every outline title in the document, depth first.
fn outline_titles(pdf: &[u8]) -> Vec<String> {
    let doc = lopdf::Document::load_mem(pdf).expect("load_mem");
    let catalog = doc.catalog().expect("catalog");
    let Ok(outlines) = catalog.get(b"Outlines").and_then(|v| v.as_reference()) else {
        return Vec::new();
    };
    fn walk(doc: &lopdf::Document, first: Option<lopdf::ObjectId>, out: &mut Vec<String>) {
        let mut cur = first;
        while let Some(id) = cur {
            let dict = doc.get_dictionary(id).expect("outline item");
            if let Ok(s) = dict.get(b"Title").and_then(|t| t.as_str()) {
                out.push(if s.starts_with(&[0xFE, 0xFF]) {
                    let units: Vec<u16> = s[2..]
                        .as_chunks::<2>()
                        .0
                        .iter()
                        .map(|&c| u16::from_be_bytes(c))
                        .collect();
                    String::from_utf16_lossy(&units)
                } else {
                    String::from_utf8_lossy(s).into_owned()
                });
            }
            walk(
                doc,
                dict.get(b"First").and_then(|v| v.as_reference()).ok(),
                out,
            );
            cur = dict.get(b"Next").and_then(|v| v.as_reference()).ok();
        }
    }
    let root = doc.get_dictionary(outlines).expect("outlines");
    let mut out = Vec::new();
    walk(
        &doc,
        root.get(b"First").and_then(|v| v.as_reference()).ok(),
        &mut out,
    );
    out
}

/// Render `marker` in every wrap with bookmarks on and assert the outline.
fn assert_outline(css: &str, marker: &str, expected: &[&str]) {
    let mut failures = Vec::new();
    for &(name, open, close) in WRAPS {
        let pdf = Engine::builder()
            .bookmarks(true)
            .build()
            .render(&html(css, (open, close), marker))
            .expect("render");
        let titles = outline_titles(&pdf);
        if titles != expected {
            failures.push(format!("{name}: expected {expected:?}, found {titles:?}"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn bookmark_on_display_none_heading_is_dropped() {
    assert_outline("", r#"<h2 style="display:none">BMTEXT</h2>"#, &[]);
}

#[test]
fn explicit_bookmark_on_display_none_is_dropped() {
    assert_outline(
        ".b { bookmark-level: 1; bookmark-label: 'BMTEXT'; display: none; }",
        r#"<div class="b"></div>"#,
        &[],
    );
}

#[test]
fn bookmark_inside_display_none_is_dropped() {
    assert_outline(
        "",
        r#"<div style="display:none"><h2>BMTEXT</h2></div>"#,
        &[],
    );
}

/// Control: the same heading as a box.
#[test]
fn bookmark_on_heading_box_is_kept() {
    assert_outline("", "<h2>BMTEXT</h2>", &["BMTEXT"]);
}

#[test]
fn bookmark_on_running_element_is_kept() {
    assert_outline("", r#"<h2 class="hdr-run">BMTEXT</h2>"#, &["BMTEXT"]);
}

#[test]
fn bookmark_on_running_element_kept_as_box_is_kept() {
    assert_outline(
        "",
        r#"<h2 class="hdr-run" style="display:block">BMTEXT</h2>"#,
        &["BMTEXT"],
    );
}
