//! A running element or `string-set` nested inside a container must still
//! reach the margin boxes.
//!
//! These markers sit on boxes with no area: GCPM rewrites `position:
//! running()` to `display: none`, a `display: none` element has no box, and
//! an empty inline `<span>` lays out 0×0. The body-level walker records such
//! a child with a zero-size fragment. A container that fits its page is not
//! walked; its descendants are recorded by `record_subtree_descendants`,
//! which skipped zero-area nodes, so the marker never entered the geometry
//! table and the per-page running / string-set walks never saw it. The
//! render succeeded with the header simply missing.
//!
//! Reference: Prince 17 shows the margin-box text on the expected page for
//! every case below, with one exception noted on `WRAPS`.

use fulgur::Engine;

const STYLE: &str = r#"
@page { size: 400px 400px; margin: 50px; }
html, body { margin: 0; }
body { font-family: sans-serif; font-size: 15px; line-height: 20px; }
p { margin: 0; }
"#;

/// Where the marker sits, and the page (1-based) the margin box must show
/// it on.
///
/// `flex`: Prince 17 drops a running element inside a flex column entirely —
/// not in the margin box and not in flow — while it honours a `string-set`
/// in the same place. GCPM 3 §2 makes `position: running()` remove the
/// element from the normal flow whatever its parent's formatting context, so
/// this follows the spec there rather than Prince.
const WRAPS: &[(&str, &str, &str, usize)] = &[
    ("body", "", "", 1),
    ("section", "<section>", "</section>", 1),
    ("two deep", "<section><div>", "</div></section>", 1),
    (
        "flex column",
        r#"<div style="display:flex;flex-direction:column">"#,
        "</div>",
        1,
    ),
    // Four 100px blocks overflow the 300px content box, so the section
    // splits and the marker lands on page 2.
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
        2,
    ),
];

/// Words per page (index 0 = page 1), via `pdftotext -bbox`. `None` (the
/// test skips) when poppler's pdftotext is not available; required on Linux
/// CI, where ci.yml installs poppler-utils.
fn page_words(pdf: &[u8]) -> Option<Vec<Vec<String>>> {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("out.pdf");
    std::fs::write(&path, pdf).unwrap();
    let required = cfg!(target_os = "linux") && std::env::var_os("CI").is_some();
    let out = match std::process::Command::new("pdftotext")
        .arg("-bbox")
        .arg(&path)
        .arg("-")
        .output()
    {
        Ok(out) if out.status.success() => out,
        Ok(out) if required => panic!("pdftotext -bbox failed on Linux CI: {out:?}"),
        Err(e) if required => panic!("pdftotext is required on Linux CI: {e}"),
        _ => return None,
    };
    let html = String::from_utf8(out.stdout).unwrap();
    let mut pages: Vec<Vec<String>> = Vec::new();
    for line in html.lines() {
        if line.contains("<page ") {
            pages.push(Vec::new());
        } else if let Some(rest) = line.trim().strip_prefix("<word ") {
            let start = rest.find('>').unwrap() + 1;
            let end = rest.rfind("</word>").unwrap();
            pages.last_mut().unwrap().push(rest[start..end].to_string());
        }
    }
    Some(pages)
}

/// Render `marker` in every wrap and assert `needle` appears exactly once,
/// on the wrap's expected page.
fn assert_margin_box_shows(css: &str, marker: &str, needle: &str) {
    let mut failures = Vec::new();
    for &(name, open, close, page) in WRAPS {
        let html = format!(
            "<!DOCTYPE html><html><head><style>{STYLE}{css}</style></head><body>\
             {open}{marker}<p>BODYTEXT</p>{close}</body></html>"
        );
        let pdf = Engine::builder().build().render(&html).expect("render");
        let Some(pages) = page_words(&pdf) else {
            return;
        };
        let found: Vec<usize> = pages
            .iter()
            .enumerate()
            .flat_map(|(i, words)| words.iter().filter(|w| *w == needle).map(move |_| i + 1))
            .collect();
        if found != [page] {
            failures.push(format!(
                "{name}: {needle} expected once on page {page}, found on pages {found:?}"
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn running_element_reaches_margin_box_from_any_depth() {
    assert_margin_box_shows(
        ".hdr { position: running(hdr); } @page { @top-center { content: element(hdr); } }",
        r#"<div class="hdr">HDRTEXT</div>"#,
        "HDRTEXT",
    );
}

#[test]
fn string_set_on_display_none_reaches_margin_box_from_any_depth() {
    assert_margin_box_shows(
        ".s { string-set: x attr(data-t); display: none; } \
         @page { @top-center { content: string(x); } }",
        r#"<div class="s" data-t="STRTEXT"></div>"#,
        "STRTEXT",
    );
}

#[test]
fn string_set_on_empty_inline_reaches_margin_box_from_any_depth() {
    assert_margin_box_shows(
        ".s { string-set: x attr(data-t); } @page { @top-center { content: string(x); } }",
        r#"<p>LEAD <span class="s" data-t="STRTEXT"></span></p>"#,
        "STRTEXT",
    );
}

/// Control: an empty block lays out with width and zero height, which was
/// always recorded. Guards the zero-height path the fix sits beside.
#[test]
fn string_set_on_empty_block_reaches_margin_box_from_any_depth() {
    assert_margin_box_shows(
        ".s { string-set: x attr(data-t); } @page { @top-center { content: string(x); } }",
        r#"<div class="s" data-t="STRTEXT"></div>"#,
        "STRTEXT",
    );
}
