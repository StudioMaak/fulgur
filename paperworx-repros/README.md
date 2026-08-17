# paperworx repros

Four defects found while evaluating fulgur as a PDF engine for
[paperworx](https://github.com/StudioMaak/paperworx), 2026-08-16. Each was measured
against WeasyPrint 69 and Chrome 151 (headless-shell) on identical input; both of those
agree with each other, so they are the reference.

Intent: fix here, then offer upstream as separate PRs.

| # | defect | file | severity | status |
|---|---|---|---|---|
| 1 | Running elements not repeated per page | `01-running-element-per-page.html` | blocks NBB | open |
| 2 | `<thead>` not repeated on continuation pages | `02-thead-repeat.html` | blocks NBB | open |
| 3 | Silent blank text when no registered font matches | `03-font-miss-silent-blank.md` | **dangerous** | open |
| 4 | Margin-box slots not anchored to their named position | `04-margin-box-anchor.html` | blocks NBB | **fixed** |
| 5 | Margin-box background does not fill the box's band | — | cosmetic | **deferred** — after 1-3 |

## 5 — deferred until 1-3 are done

Found while checking whether 4's fix was a general capability or only enough for our own
documents. It is a real conformance gap, but nothing paperworx renders depends on it, so
it waits.

A red `background` on `@bottom-center`, A4/25mm (band = x 25-185mm, y 272-297mm):

| engine | horizontal | vertical |
|---|---|---|
| WeasyPrint 69 | 103.0-105.8mm (shrink-wrapped to the text) | 272.3-296.3mm (full band) |
| fulgur | 25.4-184.1mm (full width) | 282.9-285.8mm (**content height only**) |
| Chrome 151 | 24.7-184.1mm (full width) | 272.3-296.3mm (full band) |

**This is the one case where the two reference engines disagree with each other**, so
their agreement cannot settle it. Chrome's model — the margin box *is* its rect, on both
axes — is the one matching §5.3.3, and it also explains the `text-align` divergence:
WeasyPrint shrink-wraps horizontally, so alignment inside the box is moot there.

The cause is that an author's `declarations` render on an inner element rather than on the
box. Moving them to the wrapper fixes the background, but the wrapper zeroes
`margin`/`padding` precisely so the renderer can paint at `rect.x, rect.y` with a (0, 0)
body offset — so author `margin` has to be handled in the same change.

## 4 — fixed

Root cause was *not* the box geometry: `compute_edge_layout` already produced the right
rect (a lone `@bottom-right` correctly spans the whole content width). What was missing is
the default `text-align` / `vertical-align` CSS Paged Media 3 §5.3.2 gives each of the
sixteen slots, so every box rendered flush to the top-left of its rect.

The alignment table was recovered by rendering each slot **alone** on A4/25mm through both
reference engines — alone on its edge a box gets that edge's whole band, so the text's
position names its alignment directly. Both engines agreed on all sixteen.

Fixed by handing the slot's alignment to the layout engine as ordinary CSS on the wrapper
document (`render.rs::margin_box_document`), keeping the existing split where fulgur owns
the rect and Blitz owns everything inside it. Covered by
`crates/fulgur/tests/margin_box_alignment.rs` (six anchor-invariant tests, all six verified
to fail before the fix) plus the alignment table in `gcpm/margin_box.rs`. The repro now
puts `MARK` at 176.34mm / 282.85mm against the references' 176.50 / 282.90 — the residual
is font-metric difference, the same order as Chrome's own 0.15mm disagreement with
WeasyPrint.

## How they were measured

```bash
# reference engines
uvx --from weasyprint weasyprint in.html out-weasy.pdf
# fulgur
cargo run -p fulgur-cli -- render -o out-ful.pdf in.html
# compare text positions in mm
pdftotext -bbox -f 1 -l 1 out.pdf -    # xMin/yMin are pt; * 25.4/72 = mm
```
