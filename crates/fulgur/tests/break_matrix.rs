//! Break matrix: where content lands right after a zero-height forced page
//! break, against reference-engine positions written down as literals.
//!
//! Every case is `PRE`, then `<div style="height:0; break-after:page">`, then
//! one axis-A item, then the markers `ZZZ`, a 30px spacer and `YYY`, all
//! wrapped in one axis-B container. Markup is pretty-printed (a newline
//! between elements) except for the `-min` kinds.
//!
//! - **Axis A** (`kind`): what follows the break. See [`axis_a`].
//! - **Axis B** (`wrap`): body level, a `<section>`, two deep, a flex column,
//!   or a one-column grid. See [`axis_b`].
//! - **Pre** (`pre`): a 60px block (`short`), or a ten-row table that spans
//!   a page first (`table`).
//!
//! # Where the expected values come from
//!
//! Measured with `pdftotext -bbox` on Prince 17 and Chrome 157 output of the
//! same documents (rendered in Liberation Sans), and copied in by hand. They
//! are never derived from fulgur output. Reference policy:
//!
//! - Prince 17 by default.
//! - Chrome inside flex/grid: Chrome honours the forced break there, Prince
//!   ignores it.
//! - Chrome for `mt30`: Chrome keeps a zero-height box's `margin-top` after a
//!   forced break, Prince drops it (while keeping `margin-bottom`).
//! - Running elements: Chrome does not implement `running()`, so `running`
//!   and `running-dnone` expect the position of the matching `dnone` case.
//! - Floats: Prince, known-bad, out of scope (a layout issue, not pagination).
//!
//! fulgur renders in bundled Noto Sans at a fixed `line-height`, so the line
//! box is font-independent; `yMin` still differs by about 1pt between
//! engines because each declares a different ascent. Tolerance: same page and
//! `|Δy| <= 3pt`.
//!
//! # Status
//!
//! A `Pass` row must match. A `KnownBug` row must *not* match: when a change
//! fixes one, this test fails and asks for the row to be promoted to `Pass`,
//! so the table always says what is fixed. Every row, whatever its status,
//! must keep all text inside the content box.
//!
//! # The first-child shapes
//!
//! [`SHAPES`] is a second, hand-picked table for the step-2 walker work: a
//! container whose first child lands in the last few px of a page. Today the
//! first child is emitted where it stands, past the content-box bottom,
//! because nothing inside the container knows that the page above it is
//! already occupied. The naive fix (local branch `wip/first-child-occupied`)
//! moved it, but also pushed a short stretched flex/grid cell a page on and
//! left a blank first page when only padding sat above the container. Those
//! shapes are `Pass` rows here and must stay `Pass`.
//!
//! Same page, fonts and tolerance as the break matrix. Prince 17 and Chrome
//! 157 agree to 0.1pt on every row; the reference column follows the same
//! policy (Chrome for flex/grid, Prince otherwise). Text past the content
//! box is a mismatch here, not a hard failure, because it is the bug.

use std::path::PathBuf;

use fulgur::Engine;
use fulgur::asset::AssetBundle;

/// Page 400px square, 50px margins: the content box ends at 350px = 262.5pt.
const CONTENT_BOTTOM_PT: f32 = 262.5;
const TOL_PT: f32 = 3.0;
const MARKERS: [&str; 2] = ["ZZZ", "YYY"];

#[derive(Clone, Copy, Debug, PartialEq)]
enum Status {
    Pass,
    KnownBug(&'static str),
}
use Status::{KnownBug, Pass};

struct Case {
    pre: &'static str,
    wrap: &'static str,
    kind: &'static str,
    /// Which reference engine the expected values were measured on.
    reference: &'static str,
    /// Expected `(page, yMin pt)` of `ZZZ` and `YYY`, pages 1-based.
    expected: [(usize, f32); 2],
    status: Status,
}

const fn row(
    pre: &'static str,
    wrap: &'static str,
    kind: &'static str,
    reference: &'static str,
    zzz: (usize, f32),
    yyy: (usize, f32),
    status: Status,
) -> Case {
    Case {
        pre,
        wrap,
        kind,
        reference,
        expected: [zzz, yyy],
        status,
    }
}

#[rustfmt::skip]
const CASES: &[Case] = &[
    row("short", "body", "control", "prince", (2, 38.7), (2, 76.2), Pass),
    row("short", "body", "dnone", "prince", (2, 38.7), (2, 76.2), Pass),
    row("short", "body", "contents", "prince", (2, 38.7), (2, 76.2), Pass),
    row("short", "body", "contents-ch", "prince", (2, 38.7), (2, 76.2), Pass),
    row("short", "body", "abs", "prince", (2, 38.7), (2, 76.2), Pass),
    row("short", "body", "fixed", "prince", (2, 38.7), (2, 76.2), Pass),
    row("short", "body", "float", "prince", (2, 38.7), (2, 76.2), KnownBug("float layout")),
    row("short", "body", "span-empty", "prince", (2, 38.7), (2, 76.2), Pass),
    row("short", "body", "comment", "prince", (2, 38.7), (2, 76.2), Pass),
    row("short", "body", "border-top", "prince", (2, 42.5), (2, 80.0), Pass),
    row("short", "body", "pad-top", "prince", (2, 46.2), (2, 83.7), Pass),
    row("short", "body", "mt30", "chrome", (2, 61.1), (2, 98.6), KnownBug("mt30 margin-top dropped")),
    row("short", "body", "mb30", "prince", (2, 61.2), (2, 98.7), KnownBug("bug4 body-level margin-bottom")),
    row("short", "body", "vis-hidden", "prince", (2, 53.7), (2, 91.2), Pass),
    row("short", "body", "z-z", "prince", (2, 38.7), (2, 76.2), Pass),
    row("short", "body", "z-zmb30", "prince", (2, 61.2), (2, 98.7), KnownBug("bug4 body-level margin-bottom")),
    row("short", "body", "dnone-mb30", "prince", (2, 61.2), (2, 98.7), KnownBug("bug4 body-level margin-bottom")),
    row("short", "body", "running", "prince, as dnone", (2, 38.7), (2, 76.2), Pass),
    row("short", "body", "running-dnone", "prince, as dnone", (2, 38.7), (2, 76.2), Pass),
    row("short", "body", "control-min", "prince", (2, 38.7), (2, 76.2), Pass),
    row("short", "body", "dnone-min", "prince", (2, 38.7), (2, 76.2), Pass),
    row("short", "section", "control", "prince", (2, 38.7), (2, 76.2), Pass),
    row("short", "section", "dnone", "prince", (2, 38.7), (2, 76.2), Pass),
    row("short", "section", "contents", "prince", (2, 38.7), (2, 76.2), Pass),
    row("short", "section", "contents-ch", "prince", (2, 38.7), (2, 76.2), Pass),
    row("short", "section", "abs", "prince", (2, 38.7), (2, 76.2), Pass),
    row("short", "section", "fixed", "prince", (2, 38.7), (2, 76.2), Pass),
    row("short", "section", "float", "prince", (2, 38.7), (2, 76.2), KnownBug("float layout")),
    row("short", "section", "span-empty", "prince", (2, 38.7), (2, 76.2), Pass),
    row("short", "section", "comment", "prince", (2, 38.7), (2, 76.2), Pass),
    row("short", "section", "border-top", "prince", (2, 42.5), (2, 80.0), Pass),
    row("short", "section", "pad-top", "prince", (2, 46.2), (2, 83.7), Pass),
    row("short", "section", "mt30", "chrome", (2, 61.1), (2, 98.6), KnownBug("mt30 margin-top dropped")),
    row("short", "section", "mb30", "prince", (2, 61.2), (2, 98.7), Pass),
    row("short", "section", "vis-hidden", "prince", (2, 53.7), (2, 91.2), Pass),
    row("short", "section", "z-z", "prince", (2, 38.7), (2, 76.2), Pass),
    row("short", "section", "z-zmb30", "prince", (2, 61.2), (2, 98.7), Pass),
    row("short", "section", "dnone-mb30", "prince", (2, 61.2), (2, 98.7), Pass),
    row("short", "section", "running", "prince, as dnone", (2, 38.7), (2, 76.2), Pass),
    row("short", "section", "running-dnone", "prince, as dnone", (2, 38.7), (2, 76.2), Pass),
    row("short", "section", "control-min", "prince", (2, 38.7), (2, 76.2), Pass),
    row("short", "section", "dnone-min", "prince", (2, 38.7), (2, 76.2), Pass),
    row("short", "nest2", "control", "prince", (2, 38.7), (2, 76.2), Pass),
    row("short", "nest2", "dnone", "prince", (2, 38.7), (2, 76.2), Pass),
    row("short", "nest2", "contents", "prince", (2, 38.7), (2, 76.2), Pass),
    row("short", "nest2", "contents-ch", "prince", (2, 38.7), (2, 76.2), Pass),
    row("short", "nest2", "abs", "prince", (2, 38.7), (2, 76.2), Pass),
    row("short", "nest2", "fixed", "prince", (2, 38.7), (2, 76.2), Pass),
    row("short", "nest2", "float", "prince", (2, 38.7), (2, 76.2), KnownBug("float layout")),
    row("short", "nest2", "span-empty", "prince", (2, 38.7), (2, 76.2), Pass),
    row("short", "nest2", "comment", "prince", (2, 38.7), (2, 76.2), Pass),
    row("short", "nest2", "border-top", "prince", (2, 42.5), (2, 80.0), Pass),
    row("short", "nest2", "pad-top", "prince", (2, 46.2), (2, 83.7), Pass),
    row("short", "nest2", "mt30", "chrome", (2, 61.1), (2, 98.6), KnownBug("mt30 margin-top dropped")),
    row("short", "nest2", "mb30", "prince", (2, 61.2), (2, 98.7), Pass),
    row("short", "nest2", "vis-hidden", "prince", (2, 53.7), (2, 91.2), Pass),
    row("short", "nest2", "z-z", "prince", (2, 38.7), (2, 76.2), Pass),
    row("short", "nest2", "z-zmb30", "prince", (2, 61.2), (2, 98.7), Pass),
    row("short", "nest2", "dnone-mb30", "prince", (2, 61.2), (2, 98.7), Pass),
    row("short", "nest2", "running", "prince, as dnone", (2, 38.7), (2, 76.2), Pass),
    row("short", "nest2", "running-dnone", "prince, as dnone", (2, 38.7), (2, 76.2), Pass),
    row("short", "nest2", "control-min", "prince", (2, 38.7), (2, 76.2), Pass),
    row("short", "nest2", "dnone-min", "prince", (2, 38.7), (2, 76.2), Pass),
    row("short", "flex", "control", "chrome", (2, 38.6), (2, 76.1), Pass),
    row("short", "flex", "dnone", "chrome", (2, 38.6), (2, 76.1), Pass),
    row("short", "flex", "contents", "chrome", (2, 38.6), (2, 76.1), Pass),
    row("short", "flex", "contents-ch", "chrome", (2, 38.6), (2, 76.1), Pass),
    row("short", "flex", "abs", "chrome", (2, 38.6), (2, 76.1), Pass),
    row("short", "flex", "fixed", "chrome", (2, 38.6), (2, 76.1), Pass),
    row("short", "flex", "float", "chrome", (2, 46.1), (2, 83.6), Pass),
    row("short", "flex", "span-empty", "chrome", (2, 38.6), (2, 76.1), Pass),
    row("short", "flex", "comment", "chrome", (2, 38.6), (2, 76.1), Pass),
    row("short", "flex", "border-top", "chrome", (2, 42.3), (2, 79.8), Pass),
    row("short", "flex", "pad-top", "chrome", (2, 46.1), (2, 83.6), Pass),
    row("short", "flex", "mt30", "chrome", (2, 61.1), (2, 98.6), KnownBug("mt30 margin-top dropped")),
    row("short", "flex", "mb30", "chrome", (2, 61.1), (2, 98.6), Pass),
    row("short", "flex", "vis-hidden", "chrome", (2, 53.6), (2, 91.1), Pass),
    row("short", "flex", "z-z", "chrome", (2, 38.6), (2, 76.1), Pass),
    row("short", "flex", "z-zmb30", "chrome", (2, 61.1), (2, 98.6), Pass),
    row("short", "flex", "dnone-mb30", "chrome", (2, 61.1), (2, 98.6), Pass),
    row("short", "flex", "running", "chrome, as dnone", (2, 38.6), (2, 76.1), Pass),
    row("short", "flex", "running-dnone", "chrome, as dnone", (2, 38.6), (2, 76.1), Pass),
    row("short", "flex", "control-min", "chrome", (2, 38.6), (2, 76.1), Pass),
    row("short", "flex", "dnone-min", "chrome", (2, 38.6), (2, 76.1), Pass),
    row("short", "grid", "control", "chrome", (2, 38.6), (2, 76.1), Pass),
    row("short", "grid", "dnone", "chrome", (2, 38.6), (2, 76.1), Pass),
    row("short", "grid", "contents", "chrome", (2, 38.6), (2, 76.1), Pass),
    row("short", "grid", "contents-ch", "chrome", (2, 38.6), (2, 76.1), Pass),
    row("short", "grid", "abs", "chrome", (2, 38.6), (2, 76.1), Pass),
    row("short", "grid", "fixed", "chrome", (2, 38.6), (2, 76.1), Pass),
    row("short", "grid", "float", "chrome", (2, 46.1), (2, 83.6), Pass),
    row("short", "grid", "span-empty", "chrome", (2, 38.6), (2, 76.1), Pass),
    row("short", "grid", "comment", "chrome", (2, 38.6), (2, 76.1), Pass),
    row("short", "grid", "border-top", "chrome", (2, 42.3), (2, 79.8), Pass),
    row("short", "grid", "pad-top", "chrome", (2, 46.1), (2, 83.6), Pass),
    row("short", "grid", "mt30", "chrome", (2, 61.1), (2, 98.6), KnownBug("mt30 margin-top dropped")),
    row("short", "grid", "mb30", "chrome", (2, 61.1), (2, 98.6), Pass),
    row("short", "grid", "vis-hidden", "chrome", (2, 53.6), (2, 91.1), Pass),
    row("short", "grid", "z-z", "chrome", (2, 38.6), (2, 76.1), Pass),
    row("short", "grid", "z-zmb30", "chrome", (2, 61.1), (2, 98.6), Pass),
    row("short", "grid", "dnone-mb30", "chrome", (2, 61.1), (2, 98.6), Pass),
    row("short", "grid", "running", "chrome, as dnone", (2, 38.6), (2, 76.1), Pass),
    row("short", "grid", "running-dnone", "chrome, as dnone", (2, 38.6), (2, 76.1), Pass),
    row("short", "grid", "control-min", "chrome", (2, 38.6), (2, 76.1), Pass),
    row("short", "grid", "dnone-min", "chrome", (2, 38.6), (2, 76.1), Pass),
    row("table", "body", "control", "prince", (3, 38.7), (3, 76.2), Pass),
    row("table", "body", "dnone", "prince", (3, 38.7), (3, 76.2), Pass),
    row("table", "body", "contents", "prince", (3, 38.7), (3, 76.2), Pass),
    row("table", "body", "contents-ch", "prince", (3, 38.7), (3, 76.2), Pass),
    row("table", "body", "abs", "prince", (3, 38.7), (3, 76.2), Pass),
    row("table", "body", "fixed", "prince", (3, 38.7), (3, 76.2), Pass),
    row("table", "body", "float", "prince", (3, 38.7), (3, 76.2), KnownBug("float layout")),
    row("table", "body", "span-empty", "prince", (3, 38.7), (3, 76.2), Pass),
    row("table", "body", "comment", "prince", (3, 38.7), (3, 76.2), Pass),
    row("table", "body", "border-top", "prince", (3, 42.5), (3, 80.0), Pass),
    row("table", "body", "pad-top", "prince", (3, 46.2), (3, 83.7), Pass),
    row("table", "body", "mt30", "chrome", (3, 61.1), (3, 98.6), KnownBug("mt30 margin-top dropped")),
    row("table", "body", "mb30", "prince", (3, 61.2), (3, 98.7), KnownBug("bug4 body-level margin-bottom")),
    row("table", "body", "vis-hidden", "prince", (3, 53.7), (3, 91.2), Pass),
    row("table", "body", "z-z", "prince", (3, 38.7), (3, 76.2), Pass),
    row("table", "body", "z-zmb30", "prince", (3, 61.2), (3, 98.7), KnownBug("bug4 body-level margin-bottom")),
    row("table", "body", "dnone-mb30", "prince", (3, 61.2), (3, 98.7), KnownBug("bug4 body-level margin-bottom")),
    row("table", "body", "running", "prince, as dnone", (3, 38.7), (3, 76.2), Pass),
    row("table", "body", "running-dnone", "prince, as dnone", (3, 38.7), (3, 76.2), Pass),
    row("table", "body", "control-min", "prince", (3, 38.7), (3, 76.2), Pass),
    row("table", "body", "dnone-min", "prince", (3, 38.7), (3, 76.2), Pass),
    row("table", "section", "control", "prince", (3, 38.7), (3, 76.2), Pass),
    row("table", "section", "dnone", "prince", (3, 38.7), (3, 76.2), Pass),
    row("table", "section", "contents", "prince", (3, 38.7), (3, 76.2), Pass),
    row("table", "section", "contents-ch", "prince", (3, 38.7), (3, 76.2), Pass),
    row("table", "section", "abs", "prince", (3, 38.7), (3, 76.2), Pass),
    row("table", "section", "fixed", "prince", (3, 38.7), (3, 76.2), Pass),
    row("table", "section", "float", "prince", (3, 38.7), (3, 76.2), KnownBug("float layout")),
    row("table", "section", "span-empty", "prince", (3, 38.7), (3, 76.2), Pass),
    row("table", "section", "comment", "prince", (3, 38.7), (3, 76.2), Pass),
    row("table", "section", "border-top", "prince", (3, 42.5), (3, 80.0), Pass),
    row("table", "section", "pad-top", "prince", (3, 46.2), (3, 83.7), Pass),
    row("table", "section", "mt30", "chrome", (3, 61.1), (3, 98.6), KnownBug("mt30 margin-top dropped")),
    row("table", "section", "mb30", "prince", (3, 61.2), (3, 98.7), Pass),
    row("table", "section", "vis-hidden", "prince", (3, 53.7), (3, 91.2), Pass),
    row("table", "section", "z-z", "prince", (3, 38.7), (3, 76.2), Pass),
    row("table", "section", "z-zmb30", "prince", (3, 61.2), (3, 98.7), Pass),
    row("table", "section", "dnone-mb30", "prince", (3, 61.2), (3, 98.7), Pass),
    row("table", "section", "running", "prince, as dnone", (3, 38.7), (3, 76.2), Pass),
    row("table", "section", "running-dnone", "prince, as dnone", (3, 38.7), (3, 76.2), Pass),
    row("table", "section", "control-min", "prince", (3, 38.7), (3, 76.2), Pass),
    row("table", "section", "dnone-min", "prince", (3, 38.7), (3, 76.2), Pass),
    row("table", "nest2", "control", "prince", (3, 38.7), (3, 76.2), Pass),
    row("table", "nest2", "dnone", "prince", (3, 38.7), (3, 76.2), Pass),
    row("table", "nest2", "contents", "prince", (3, 38.7), (3, 76.2), Pass),
    row("table", "nest2", "contents-ch", "prince", (3, 38.7), (3, 76.2), Pass),
    row("table", "nest2", "abs", "prince", (3, 38.7), (3, 76.2), Pass),
    row("table", "nest2", "fixed", "prince", (3, 38.7), (3, 76.2), Pass),
    row("table", "nest2", "float", "prince", (3, 38.7), (3, 76.2), KnownBug("float layout")),
    row("table", "nest2", "span-empty", "prince", (3, 38.7), (3, 76.2), Pass),
    row("table", "nest2", "comment", "prince", (3, 38.7), (3, 76.2), Pass),
    row("table", "nest2", "border-top", "prince", (3, 42.5), (3, 80.0), Pass),
    row("table", "nest2", "pad-top", "prince", (3, 46.2), (3, 83.7), Pass),
    row("table", "nest2", "mt30", "chrome", (3, 61.1), (3, 98.6), KnownBug("mt30 margin-top dropped")),
    row("table", "nest2", "mb30", "prince", (3, 61.2), (3, 98.7), Pass),
    row("table", "nest2", "vis-hidden", "prince", (3, 53.7), (3, 91.2), Pass),
    row("table", "nest2", "z-z", "prince", (3, 38.7), (3, 76.2), Pass),
    row("table", "nest2", "z-zmb30", "prince", (3, 61.2), (3, 98.7), Pass),
    row("table", "nest2", "dnone-mb30", "prince", (3, 61.2), (3, 98.7), Pass),
    row("table", "nest2", "running", "prince, as dnone", (3, 38.7), (3, 76.2), Pass),
    row("table", "nest2", "running-dnone", "prince, as dnone", (3, 38.7), (3, 76.2), Pass),
    row("table", "nest2", "control-min", "prince", (3, 38.7), (3, 76.2), Pass),
    row("table", "nest2", "dnone-min", "prince", (3, 38.7), (3, 76.2), Pass),
    row("table", "flex", "control", "chrome", (3, 38.6), (3, 76.1), Pass),
    row("table", "flex", "dnone", "chrome", (3, 38.6), (3, 76.1), Pass),
    row("table", "flex", "contents", "chrome", (3, 38.6), (3, 76.1), Pass),
    row("table", "flex", "contents-ch", "chrome", (3, 38.6), (3, 76.1), Pass),
    row("table", "flex", "abs", "chrome", (3, 38.6), (3, 76.1), Pass),
    row("table", "flex", "fixed", "chrome", (3, 38.6), (3, 76.1), Pass),
    row("table", "flex", "float", "chrome", (3, 46.1), (3, 83.6), Pass),
    row("table", "flex", "span-empty", "chrome", (3, 38.6), (3, 76.1), Pass),
    row("table", "flex", "comment", "chrome", (3, 38.6), (3, 76.1), Pass),
    row("table", "flex", "border-top", "chrome", (3, 42.3), (3, 79.8), Pass),
    row("table", "flex", "pad-top", "chrome", (3, 46.1), (3, 83.6), Pass),
    row("table", "flex", "mt30", "chrome", (3, 61.1), (3, 98.6), KnownBug("mt30 margin-top dropped")),
    row("table", "flex", "mb30", "chrome", (3, 61.1), (3, 98.6), Pass),
    row("table", "flex", "vis-hidden", "chrome", (3, 53.6), (3, 91.1), Pass),
    row("table", "flex", "z-z", "chrome", (3, 38.6), (3, 76.1), Pass),
    row("table", "flex", "z-zmb30", "chrome", (3, 61.1), (3, 98.6), Pass),
    row("table", "flex", "dnone-mb30", "chrome", (3, 61.1), (3, 98.6), Pass),
    row("table", "flex", "running", "chrome, as dnone", (3, 38.6), (3, 76.1), Pass),
    row("table", "flex", "running-dnone", "chrome, as dnone", (3, 38.6), (3, 76.1), Pass),
    row("table", "flex", "control-min", "chrome", (3, 38.6), (3, 76.1), Pass),
    row("table", "flex", "dnone-min", "chrome", (3, 38.6), (3, 76.1), Pass),
    row("table", "grid", "control", "chrome", (3, 38.6), (3, 76.1), Pass),
    row("table", "grid", "dnone", "chrome", (3, 38.6), (3, 76.1), Pass),
    row("table", "grid", "contents", "chrome", (3, 38.6), (3, 76.1), Pass),
    row("table", "grid", "contents-ch", "chrome", (3, 38.6), (3, 76.1), Pass),
    row("table", "grid", "abs", "chrome", (3, 38.6), (3, 76.1), Pass),
    row("table", "grid", "fixed", "chrome", (3, 38.6), (3, 76.1), Pass),
    row("table", "grid", "float", "chrome", (3, 46.1), (3, 83.6), Pass),
    row("table", "grid", "span-empty", "chrome", (3, 38.6), (3, 76.1), Pass),
    row("table", "grid", "comment", "chrome", (3, 38.6), (3, 76.1), Pass),
    row("table", "grid", "border-top", "chrome", (3, 42.3), (3, 79.8), Pass),
    row("table", "grid", "pad-top", "chrome", (3, 46.1), (3, 83.6), Pass),
    row("table", "grid", "mt30", "chrome", (3, 61.1), (3, 98.6), KnownBug("mt30 margin-top dropped")),
    row("table", "grid", "mb30", "chrome", (3, 61.1), (3, 98.6), Pass),
    row("table", "grid", "vis-hidden", "chrome", (3, 53.6), (3, 91.1), Pass),
    row("table", "grid", "z-z", "chrome", (3, 38.6), (3, 76.1), Pass),
    row("table", "grid", "z-zmb30", "chrome", (3, 61.1), (3, 98.6), Pass),
    row("table", "grid", "dnone-mb30", "chrome", (3, 61.1), (3, 98.6), Pass),
    row("table", "grid", "running", "chrome, as dnone", (3, 38.6), (3, 76.1), Pass),
    row("table", "grid", "running-dnone", "chrome, as dnone", (3, 38.6), (3, 76.1), Pass),
    row("table", "grid", "control-min", "chrome", (3, 38.6), (3, 76.1), Pass),
    row("table", "grid", "dnone-min", "chrome", (3, 38.6), (3, 76.1), Pass),
];

/// Axis A: what follows the break div.
fn axis_a(kind: &str) -> &'static str {
    match kind {
        "control" => "",
        "dnone" => r#"<div style="display:none">NONE</div>"#,
        "contents" => r#"<div style="display:contents"></div>"#,
        "contents-ch" => r#"<div style="display:contents"><div style="height:0"></div></div>"#,
        "abs" => r#"<div style="position:absolute;width:10px;height:10px;background:red"></div>"#,
        "fixed" => {
            r#"<div style="position:fixed;top:0;left:0;width:10px;height:10px;background:red"></div>"#
        }
        "float" => r#"<div style="float:left;width:10px;height:10px;background:red"></div>"#,
        "span-empty" => "<span></span>",
        "comment" => "<!-- a comment -->",
        "border-top" => r#"<div style="border-top:5px solid black"></div>"#,
        "pad-top" => r#"<div style="padding-top:10px"></div>"#,
        "mt30" => r#"<div style="height:0;margin-top:30px"></div>"#,
        "mb30" => r#"<div style="height:0;margin-bottom:30px"></div>"#,
        "vis-hidden" => r#"<div style="visibility:hidden;height:20px">VVV</div>"#,
        "z-z" => "<div style=\"height:0\"></div>\n<div style=\"height:0\"></div>",
        "z-zmb30" => {
            "<div style=\"height:0\"></div>\n<div style=\"height:0;margin-bottom:30px\"></div>"
        }
        "dnone-mb30" => {
            "<div style=\"display:none\">NONE</div>\n<div style=\"height:0;margin-bottom:30px\"></div>"
        }
        "running" => r#"<div class="run">HDR</div>"#,
        "running-dnone" => "<div class=\"run\">HDR</div>\n<div style=\"display:none\">NONE</div>",
        other => panic!("unknown axis-A kind {other}"),
    }
}

/// Axis B: the wrapper's open and close tags.
fn axis_b(wrap: &str) -> (&'static str, &'static str) {
    match wrap {
        "body" => ("", ""),
        "section" => ("<section>", "</section>"),
        "nest2" => ("<section>\n<div>", "</div>\n</section>"),
        "flex" => (
            r#"<div style="display:flex;flex-direction:column">"#,
            "</div>",
        ),
        "grid" => (
            r#"<div style="display:grid;grid-template-columns:1fr">"#,
            "</div>",
        ),
        other => panic!("unknown axis-B wrap {other}"),
    }
}

fn pre_content(pre: &str) -> String {
    match pre {
        "short" => r#"<div style="height:60px;background:#ddd">AAA</div>"#.to_string(),
        "table" => {
            let rows: Vec<String> = (1..=10)
                .map(|i| {
                    format!(r#"<tr style="height:40px"><td style="height:40px;padding:0">R{i}</td></tr>"#)
                })
                .collect();
            format!(
                "<table style=\"border-collapse:collapse;width:100%\">\n{}\n</table>",
                rows.join("\n")
            )
        }
        other => panic!("unknown pre {other}"),
    }
}

const HEAD: &str = r#"<!DOCTYPE html>
<html>
<head>
<style>
@page { size: 400px 400px; margin: 50px; @top-center { content: element(hdr) } }
html, body { margin: 0; padding: 0; }
body { font-family: 'Noto Sans'; font-size: 15px; line-height: 20px; }
.m { height: 20px; }
.run { position: running(hdr); }
</style>
</head>
<body>
"#;

fn build(case: &Case) -> String {
    let (kind, minified) = match case.kind.strip_suffix("-min") {
        Some(base) => (base, true),
        None => (case.kind, false),
    };
    let (open, close) = axis_b(case.wrap);
    let pre = pre_content(case.pre);
    let parts = [
        open,
        &pre,
        r#"<div style="height:0; break-after:page"></div>"#,
        axis_a(kind),
        r#"<div class="m">ZZZ</div>"#,
        r#"<div style="height:30px"></div>"#,
        r#"<div class="m">YYY</div>"#,
        close,
    ];
    let parts: Vec<&str> = parts.into_iter().filter(|p| !p.is_empty()).collect();
    let body = if minified {
        strip_inter_tag_whitespace(&parts.concat())
    } else {
        parts.join("\n")
    };
    format!("{HEAD}{body}\n</body>\n</html>\n")
}

/// Drops whitespace that sits alone between `>` and `<`.
fn strip_inter_tag_whitespace(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut pending = String::new();
    let mut after_gt = false;
    for c in s.chars() {
        if after_gt && c.is_whitespace() {
            pending.push(c);
            continue;
        }
        if c != '<' {
            out.push_str(&pending);
        }
        pending.clear();
        out.push(c);
        after_gt = c == '>';
    }
    out.push_str(&pending);
    out
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

struct Word {
    page: usize,
    y_min: f32,
    y_max: f32,
    text: String,
}

/// Words with their page (1-based) and bbox in pt, via `pdftotext -bbox`.
/// `None` (the test skips) when poppler's pdftotext is not available.
fn words(pdf: &[u8]) -> Option<Vec<Word>> {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("out.pdf");
    std::fs::write(&path, pdf).unwrap();
    // Only poppler's pdftotext has `-bbox`; the Windows CI cell ships xpdf's,
    // which rejects it (exit 99). Skip whenever bbox output is unavailable,
    // except on Linux CI, where ci.yml installs poppler-utils and a skip would
    // hide a lost regression check.
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
                y_max: attr("yMax"),
                text: rest[text_start..text_end].to_string(),
            });
        }
    }
    Some(words)
}

fn id(case: &Case) -> String {
    format!("{}.{}.{}", case.pre, case.wrap, case.kind)
}

/// Renders `html` and reads its words back. `Ok(None)` when pdftotext is
/// unavailable locally.
fn render_words(html: &str, assets: &AssetBundle) -> Result<Option<Vec<Word>>, Vec<String>> {
    let pdf = Engine::builder()
        .assets(assets.clone())
        .build()
        .render(html)
        .map_err(|e| vec![format!("render failed: {e}")])?;
    Ok(words(&pdf))
}

/// Words whose bbox ends below the content box.
fn overflow(words: &[Word]) -> Vec<String> {
    words
        .iter()
        .filter(|w| w.y_max > CONTENT_BOTTOM_PT + 0.5)
        .map(|w| format!("{} on page {} has yMax {:.1}", w.text, w.page, w.y_max))
        .collect()
}

/// Where each `(marker, page, yMin)` differs from `words`.
fn marker_mismatches<'a>(
    words: &[Word],
    expected: impl IntoIterator<Item = (&'a str, usize, f32)>,
) -> Vec<String> {
    let mut mismatches = Vec::new();
    for (marker, page, y) in expected {
        match words.iter().find(|w| w.text == marker) {
            None => mismatches.push(format!("{marker} missing, want p{page} {y:.1}")),
            Some(w) if w.page != page || (w.y_min - y).abs() > TOL_PT => mismatches.push(format!(
                "{marker} at p{} {:.1}, want p{page} {y:.1}",
                w.page, w.y_min
            )),
            Some(_) => {}
        }
    }
    mismatches
}

/// Renders one case. `Ok(None)` when pdftotext is unavailable locally.
/// `Err` lists problems that fail the test whatever the row's status.
/// `Ok(Some(mismatches))` lists where the markers differ from the reference.
fn run(case: &Case, assets: &AssetBundle) -> Result<Option<Vec<String>>, Vec<String>> {
    let Some(words) = render_words(&build(case), assets)? else {
        return Ok(None);
    };
    let overflow = overflow(&words);
    if !overflow.is_empty() {
        return Err(vec![format!("text past the content box: {overflow:?}")]);
    }
    let expected = MARKERS
        .iter()
        .zip(case.expected)
        .map(|(m, (page, y))| (*m, page, y));
    Ok(Some(marker_mismatches(&words, expected)))
}

type RowResult = Result<Option<Vec<String>>, Vec<String>>;

/// Runs `run` over `rows` on every core, results in row order.
fn run_parallel<T: Sync>(rows: &[T], run: impl Fn(&T) -> RowResult + Sync) -> Vec<RowResult> {
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get());
    let run = &run;
    std::thread::scope(|s| {
        let handles: Vec<_> = (0..threads)
            .map(|t| {
                s.spawn(move || {
                    rows.iter()
                        .enumerate()
                        .skip(t)
                        .step_by(threads)
                        .map(|(i, r)| (i, run(r)))
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        let mut all: Vec<_> = handles
            .into_iter()
            .flat_map(|h| h.join().unwrap())
            .collect();
        all.sort_by_key(|(i, _)| *i);
        all.into_iter().map(|(_, r)| r).collect()
    })
}

/// Checks every row's result against its status. `None` when pdftotext is
/// not installed locally.
fn failures(
    rows: impl IntoIterator<Item = (String, &'static str, Status, RowResult)>,
) -> Option<Vec<String>> {
    let mut failures = Vec::new();
    for (id, reference, status, result) in rows {
        match (result, status) {
            (Ok(None), _) => return None,
            (Err(errors), _) => failures.push(format!("{id}: {}", errors.join("; "))),
            (Ok(Some(m)), Pass) if !m.is_empty() => {
                failures.push(format!("{id} [Pass, ref {reference}]: {}", m.join("; ")))
            }
            (Ok(Some(m)), KnownBug(label)) if m.is_empty() => failures.push(format!(
                "{id} [KnownBug \"{label}\", ref {reference}]: now matches the reference; promote it to Pass"
            )),
            _ => {}
        }
    }
    Some(failures)
}

#[test]
fn break_matrix() {
    let assets = noto_assets();
    let results = run_parallel(CASES, |c| run(c, &assets));
    let rows = CASES
        .iter()
        .zip(results)
        .map(|(c, r)| (id(c), c.reference, c.status, r));
    let Some(failures) = failures(rows) else {
        return; // pdftotext not installed locally
    };
    assert!(
        failures.is_empty(),
        "{} of {} break-matrix rows failed:\n  {}",
        failures.len(),
        CASES.len(),
        failures.join("\n  ")
    );
}

/// The table itself: one row per axis combination, unique ids.
#[test]
fn break_matrix_table_is_complete() {
    let mut ids: Vec<String> = CASES.iter().map(id).collect();
    ids.sort();
    let n = ids.len();
    ids.dedup();
    assert_eq!(ids.len(), n, "duplicate case ids");
    assert_eq!(n, 2 * 5 * 21, "pre x wrap x kind");
}

/// A first-child shape: `id` is `<shape>` or `<shape>.<aspect>`, where
/// `<shape>` names the body in [`shape_body`] and `.<aspect>` splits one
/// document into rows with different statuses.
struct Shape {
    id: &'static str,
    reference: &'static str,
    /// Expected `(marker, page, yMin pt)`, pages 1-based.
    expected: &'static [(&'static str, usize, f32)],
    status: Status,
}

const fn shape(
    id: &'static str,
    reference: &'static str,
    expected: &'static [(&'static str, usize, f32)],
    status: Status,
) -> Shape {
    Shape {
        id,
        reference,
        expected,
        status,
    }
}

const FIRST_CHILD: Status = KnownBug("first child never moves");

/// Measured on Prince 17 and Chrome 157, which agree to 0.1pt on every row.
#[rustfmt::skip]
const SHAPES: &[Shape] = &[
    // (a) The first row / first child starts 10px above the content-box
    // bottom and must open page 2. Today it is drawn at p1 255pt, past the
    // bottom.
    shape("table-body", "prince", &[("ROW1", 2, 38.7), ("ROW2", 2, 68.7)], FIRST_CHILD),
    shape("table-section", "prince", &[("ROW1", 2, 38.7), ("ROW2", 2, 68.7)], FIRST_CHILD),
    shape("block-body", "prince", &[("FIRST", 2, 38.7), ("SECOND", 2, 53.7)], FIRST_CHILD),
    shape("block-section", "prince", &[("FIRST", 2, 38.7), ("SECOND", 2, 53.7)], FIRST_CHILD),
    // (b) A row whose tall left cell crosses the page: the short stretched
    // right cell stays at the row's top on page 1. The naive fix moved
    // RIGHT to p2.
    shape("flex-body", "chrome", &[("RIGHT", 1, 188.6), ("LEFT4", 2, 38.6)], Pass),
    shape("flex-section", "chrome", &[("RIGHT", 1, 188.6), ("LEFT4", 2, 38.6)], Pass),
    shape("grid-body", "chrome", &[("RIGHT", 1, 188.6), ("LEFT4", 2, 38.6)], Pass),
    shape("grid-section", "chrome", &[("RIGHT", 1, 188.6), ("LEFT4", 2, 38.6)], Pass),
    // The row's end after the split loses the last left `<p>`'s 10px
    // `margin-bottom`: AFTER at p2 142.5pt.
    shape("flex-body.after", "chrome", &[("AFTER", 2, 151.1)], KnownBug("row end drops the cell's last margin-bottom")),
    shape("grid-body.after", "chrome", &[("AFTER", 2, 151.1)], KnownBug("row end drops the cell's last margin-bottom")),
    // (b) Only the container's padding sits above its first child on page
    // 1. Whether the first child fits or not, page 1 must not be left blank.
    shape("pad-fits-body", "prince", &[("FIRST", 1, 53.7), ("SECOND", 1, 203.7)], Pass),
    shape("pad-fits-section", "prince", &[("FIRST", 1, 53.7), ("SECOND", 1, 203.7)], Pass),
    shape("pad-lines-body", "prince", &[("FIRST", 1, 53.7), ("LAST", 2, 38.7)], Pass),
    shape("pad-lines-section", "prince", &[("FIRST", 1, 53.7), ("LAST", 2, 38.7)], Pass),
    // The reviewer's shape: `body { padding-top }`, then a 290px leaf that
    // does not fit. The naive fix moved FIRST to p2 and left p1 blank.
    shape("pad-body-padding.first", "prince", &[("FIRST", 1, 53.7)], Pass),
    // Both references split the leaf; today it is drawn whole past the
    // bottom, so SECOND opens p2 10px early (37.5pt).
    shape("pad-body-padding.second", "prince", &[("SECOND", 2, 46.2)], FIRST_CHILD),
    // A childless leaf that does not fit is moved whole instead of split,
    // even when only padding is above it: page 1 is left blank today.
    shape("pad-leaf-body", "prince", &[("FIRST", 1, 53.7), ("SECOND", 2, 46.2)], KnownBug("leaf block moves instead of splitting")),
    shape("pad-leaf-section", "prince", &[("FIRST", 1, 53.7), ("SECOND", 2, 46.2)], KnownBug("leaf block moves instead of splitting")),
    // Controls: everything fits on page 1.
    shape("ctl-table", "prince", &[("ROW1", 1, 113.7), ("ROW3", 1, 173.7)], Pass),
    shape("ctl-block", "prince", &[("FIRST", 1, 113.7), ("SECOND", 1, 128.7)], Pass),
    shape("ctl-flex", "chrome", &[("RIGHT", 1, 113.6), ("AFTER", 1, 158.6)], Pass),
    shape("ctl-grid", "chrome", &[("RIGHT", 1, 113.6), ("AFTER", 1, 158.6)], Pass),
];

const SHAPE_HEAD: &str = r#"<!DOCTYPE html>
<html>
<head>
<style>
@page { size: 400px 400px; margin: 50px; }
html, body { margin: 0; padding: 0; }
body { font-family: 'Noto Sans'; font-size: 15px; line-height: 20px; }
.m { height: 20px; }
p { margin: 0 0 10px; }
table { border-collapse: collapse; width: 100%; }
td { height: 40px; padding: 0; vertical-align: top; }
</style>
</head>
<body>
"#;

/// `n` lines of `.m` markers, named by `name(i)` for `i` in `1..=n`.
fn lines(n: usize, name: impl Fn(usize) -> String) -> Vec<String> {
    (1..=n)
        .map(|i| format!(r#"<div class="m">{}</div>"#, name(i)))
        .collect()
}

/// The body of `shape` (the part of the id before any `.`).
fn shape_body(shape: &str) -> String {
    let rows = (1..=3)
        .map(|i| format!("<tr><td>ROW{i}</td></tr>"))
        .collect::<Vec<_>>()
        .join("\n");
    let table = format!("<table>\n{rows}\n</table>");
    let pair = lines(2, |i| ["FIRST", "SECOND"][i - 1].to_string()).join("\n");
    let block = format!("<div>\n{pair}\n</div>");
    let left = |n: usize| {
        (1..=n)
            .map(|i| format!("<p>LEFT{i}</p>"))
            .collect::<Vec<_>>()
            .join("\n")
    };
    let right = r#"<div class="m">RIGHT</div>"#;
    let after = r#"<div class="m">AFTER</div>"#;
    let flex = |n: usize| {
        format!(
            "<div style=\"display:flex\">\n<div style=\"width:150px\">\n{}\n</div>\n<div style=\"width:150px\">{right}</div>\n</div>\n{after}",
            left(n)
        )
    };
    let grid = |n: usize| {
        format!(
            "<div style=\"display:grid;grid-template-columns:150px 150px\">\n<div>\n{}\n</div>\n<div>{right}</div>\n</div>\n{after}",
            left(n)
        )
    };
    let section = |inner: String| format!("<section>\n{inner}\n</section>");
    let top = |px: u32| format!(r#"<div style="height:{px}px">TOP</div>"#);
    let pad = |inner: &str| format!("<div style=\"padding-top:20px\">\n{inner}\n</div>");
    let pad_fits = pad(concat!(
        "<div style=\"height:200px\">FIRST</div>\n",
        "<div class=\"m\">SECOND</div>\n",
        "<div style=\"height:100px\"></div>\n",
        "<div class=\"m\">THIRD</div>"
    ));
    let fifteen = lines(15, |i| match i {
        1 => "FIRST".to_string(),
        15 => "LAST".to_string(),
        _ => format!("L{i:02}"),
    })
    .join("\n");
    let pad_lines = pad(&format!("<div>\n{fifteen}\n</div>"));
    let leaf = "<div style=\"height:290px\">FIRST</div>\n<div class=\"m\">SECOND</div>";
    match shape {
        "table-body" => format!("{}\n{table}", top(290)),
        "table-section" => section(format!("{}\n{table}", top(290))),
        "block-body" => format!("{}\n{block}", top(290)),
        "block-section" => section(format!("{}\n{block}", top(290))),
        "flex-body" => format!("{}\n{}", top(200), flex(8)),
        "flex-section" => section(format!("{}\n{}", top(200), flex(8))),
        "grid-body" => format!("{}\n{}", top(200), grid(8)),
        "grid-section" => section(format!("{}\n{}", top(200), grid(8))),
        "pad-fits-body" => pad_fits,
        "pad-fits-section" => section(pad_fits),
        "pad-lines-body" => pad_lines,
        "pad-lines-section" => section(pad_lines),
        "pad-leaf-body" => pad(leaf),
        "pad-leaf-section" => section(pad(leaf)),
        "pad-body-padding" => {
            format!("<style>body {{ padding-top: 20px; }}</style>\n<div>\n{leaf}\n</div>")
        }
        "ctl-table" => format!("{}\n{}", top(100), section(table)),
        "ctl-block" => format!("{}\n{}", top(100), section(block)),
        "ctl-flex" => format!("{}\n{}", top(100), flex(2)),
        "ctl-grid" => format!("{}\n{}", top(100), grid(2)),
        other => panic!("unknown shape {other}"),
    }
}

fn build_shape(shape: &Shape) -> String {
    let name = shape.id.split('.').next().unwrap();
    format!("{SHAPE_HEAD}{}\n</body>\n</html>\n", shape_body(name))
}

/// Like [`run`], but text past the content box is a mismatch rather than a
/// hard failure: for the first-child shapes it is the bug itself.
fn run_shape(shape: &Shape, assets: &AssetBundle) -> RowResult {
    let Some(words) = render_words(&build_shape(shape), assets)? else {
        return Ok(None);
    };
    let mut mismatches = overflow(&words);
    mismatches.extend(marker_mismatches(&words, shape.expected.iter().copied()));
    Ok(Some(mismatches))
}

#[test]
fn first_child_shapes() {
    let assets = noto_assets();
    let results = run_parallel(SHAPES, |s| run_shape(s, &assets));
    let rows = SHAPES
        .iter()
        .zip(results)
        .map(|(s, r)| (s.id.to_string(), s.reference, s.status, r));
    let Some(failures) = failures(rows) else {
        return; // pdftotext not installed locally
    };
    assert!(
        failures.is_empty(),
        "{} of {} first-child rows failed:\n  {}",
        failures.len(),
        SHAPES.len(),
        failures.join("\n  ")
    );
}

#[test]
fn first_child_shape_ids_are_unique() {
    let mut ids: Vec<&str> = SHAPES.iter().map(|s| s.id).collect();
    ids.sort();
    let n = ids.len();
    ids.dedup();
    assert_eq!(ids.len(), n, "duplicate shape ids");
}

/// Dumps each shape's document to `$FULGUR_DUMP_SHAPES/<shape>.html`, to
/// feed the same bytes to the reference engines.
#[test]
fn dump_first_child_shapes() {
    let Some(dir) = std::env::var_os("FULGUR_DUMP_SHAPES") else {
        return;
    };
    for s in SHAPES {
        let name = s.id.split('.').next().unwrap();
        std::fs::write(
            PathBuf::from(&dir).join(format!("{name}.html")),
            build_shape(s),
        )
        .unwrap();
    }
}
