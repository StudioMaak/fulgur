//! A box broken across pages draws its border and padding only at the edges
//! it has (css-break-3 §5.4), counts them in its height, and draws a list
//! marker once. Positions are read back with `pdftotext -bbox` and compared
//! as relations, so the font's ascent cancels out. Reference engine: Chrome
//! 157, whose baselines for these documents are 770.2 / 68.2 / 350.2pt
//! (fulgur sits a constant 1.0pt higher).

use std::path::PathBuf;

use fulgur::Engine;
use fulgur::asset::AssetBundle;

const LINE_PT: f32 = 15.0; // 20px
const INSET_PT: f32 = 12.0; // 12px padding + 4px border = 16px
const TOL_PT: f32 = 0.2;

struct Word {
    page: usize,
    y_min: f32,
    text: String,
}

fn noto_assets() -> AssetBundle {
    let font_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/.fonts/NotoSans-Regular.ttf");
    let mut assets = AssetBundle::default();
    assets
        .add_font_file(&font_path)
        .unwrap_or_else(|e| panic!("failed to load Noto Sans: {e}"));
    assets
}

/// `None` (the test skips) when poppler's `pdftotext -bbox` is unavailable,
/// except on Linux CI where a skip would hide a lost regression check.
fn words(pdf: &[u8]) -> Option<Vec<Word>> {
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
    let mut page = 0;
    let mut words = Vec::new();
    for line in html.lines() {
        if line.contains("<page ") {
            page += 1;
        } else if let Some(rest) = line.trim().strip_prefix("<word ") {
            let attr = |name: &str| -> f32 {
                let start = rest.find(&format!("{name}=\"")).unwrap() + name.len() + 2;
                let end = start + rest[start..].find('"').unwrap();
                rest[start..end].parse().unwrap()
            };
            let text_start = rest.find('>').unwrap() + 1;
            let text_end = rest.rfind("</word>").unwrap();
            words.push(Word {
                page,
                y_min: attr("yMin"),
                text: rest[text_start..text_end].to_string(),
            });
        }
    }
    Some(words)
}

fn render(body: &str) -> Option<Vec<Word>> {
    let html = format!(
        "<!doctype html><html><head><style>\
         @page{{size:A4;margin:20mm}}\
         body{{margin:0;font:16px/20px sans-serif}}\
         p,li,div,ul{{margin:0}} ul{{padding:0}}\
         .f{{height:700px}} .b{{padding:12px 8px;border:4px solid #c00}}\
         </style></head><body>{body}</body></html>"
    );
    let pdf = Engine::builder()
        .assets(noto_assets())
        .build()
        .render(&html)
        .expect("render");
    words(&pdf)
}

fn lines(n: usize) -> String {
    (0..n)
        .map(|i| format!("L{i:02}"))
        .collect::<Vec<_>>()
        .join("<br>")
}

fn find<'a>(words: &'a [Word], text: &str) -> &'a Word {
    words
        .iter()
        .find(|w| w.text == text)
        .unwrap_or_else(|| panic!("word {text} not found"))
}

fn close(a: f32, b: f32, what: &str) {
    assert!((a - b).abs() <= TOL_PT, "{what}: {a} vs {b}");
}

#[test]
fn nested_padded_paragraph_splits_without_repeating_edges() {
    let body = format!(
        "<div class=\"f\">TOP</div><section><p class=\"b\">{}</p><div>AFTER</div></section>",
        lines(30)
    );
    // Control: the same page geometry with nothing padded; its first line on
    // a page is where the content box starts.
    let control = format!("<div class=\"f\">TOP</div><p>{}</p>", lines(30));
    let (Some(w), Some(c)) = (render(&body), render(&control)) else {
        return;
    };
    let content_top = find(&c, "TOP").y_min;

    assert_eq!(find(&w, "L11").page, 1, "page 1 holds L00..L11");
    assert_eq!(find(&w, "L12").page, 2, "page 2 starts with L12");
    close(
        find(&w, "L12").y_min,
        content_top,
        "L12 sits at the content top: no padding or border repeated",
    );
    let l29 = find(&w, "L29");
    let after = find(&w, "AFTER");
    assert_eq!(after.page, l29.page);
    close(
        after.y_min - l29.y_min,
        LINE_PT + INSET_PT,
        "AFTER follows the last line plus the bottom inset",
    );
}

#[test]
fn padded_paragraph_that_fits_pushes_next_sibling_past_its_insets() {
    let body = format!(
        "<div style=\"height:100px\">TOP</div><p class=\"b\">{}</p><div>AFTER</div>",
        lines(5)
    );
    let Some(w) = render(&body) else { return };
    let last = find(&w, "L04");
    let after = find(&w, "AFTER");
    assert_eq!(after.page, last.page);
    close(
        after.y_min - last.y_min,
        LINE_PT + INSET_PT,
        "AFTER sits below the bottom padding and border",
    );
}

#[test]
fn split_list_item_draws_its_marker_once() {
    let body = format!(
        "<div class=\"f\">TOP</div><ul><li>{}</li><li>NEXT</li></ul>",
        lines(30)
    );
    let Some(w) = render(&body) else { return };
    // pdftotext glues the bullet to the first word of its line ("•L00").
    let ends = |text: &str| page_of_word_ending(&w, text);
    // The first item has 30 lines and breaks across pages 1 and 2.
    assert_eq!(ends("L00"), 1);
    assert_eq!(ends("L29"), 2);
    assert_eq!(ends("NEXT"), 2);
    let markers = |page: usize| markers_on(&w, page);
    assert_eq!(markers(1), 1, "marker on the page the item starts");
    // Page 2 carries only NEXT's marker, not a repeat for the first item.
    assert_eq!(markers(2), 1, "no second marker for the continuation");
}

/// The page of the first word ending in `text`.
fn page_of_word_ending(words: &[Word], text: &str) -> usize {
    words.iter().find(|w| w.text.ends_with(text)).unwrap().page
}

/// How many bullet markers `page` carries.
fn markers_on(words: &[Word], page: usize) -> usize {
    words
        .iter()
        .filter(|w| w.page == page && w.text.contains('\u{2022}'))
        .count()
}

#[test]
fn split_list_item_holding_a_paragraph_draws_its_marker_once() {
    // The item is a container here, not an inline root: its edges come from
    // the pass that marks every box the walk broke.
    let body = format!(
        "<div class=\"f\">TOP</div><ul><li><p>{}</p></li><li>NEXT</li></ul>",
        lines(30)
    );
    let Some(w) = render(&body) else { return };
    let ends = |text: &str| page_of_word_ending(&w, text);
    assert_eq!(ends("L00"), 1);
    assert_eq!(ends("L29"), 2);
    let markers = |page: usize| markers_on(&w, page);
    assert_eq!(markers(1), 1, "marker on the page the item starts");
    assert_eq!(markers(2), 1, "only NEXT's marker on the continuation page");
}
