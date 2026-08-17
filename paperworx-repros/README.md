# paperworx repros

Four defects found while evaluating fulgur as a PDF engine for
[paperworx](https://github.com/StudioMaak/paperworx), 2026-08-16. Each was measured
against WeasyPrint 69 and Chrome 151 (headless-shell) on identical input; both of those
agree with each other, so they are the reference.

Intent: fix here, then offer upstream as separate PRs.

| # | defect | file | severity | status |
|---|---|---|---|---|
| 1 | Running elements not repeated per page | `01-running-element-per-page.html` | blocks NBB | **misdiagnosed** — see below |
| 2 | `<thead>` not repeated on continuation pages | `02-thead-repeat.html` | blocks NBB | open |
| 3 | Silent blank text when no registered font matches | `03-font-miss-silent-blank.md` | **dangerous** | **fixed** |
| 4 | Margin-box slots not anchored to their named position | `04-margin-box-anchor.html` | blocks NBB | **fixed** |
| 5 | Margin-box background does not fill the box's band | — | cosmetic | **deferred** — after 1-3 |

## 1 — misdiagnosed; the real fault was different

**Running elements were never the problem.** Re-measured at the fork point `682bcbf3`,
`RUNHEAD` was already on *both* pages — it was drawn at **y = 0.3mm**, a third of a
millimetre from the paper edge, inside the non-printable margin. That is defect 4's
signature exactly (every margin box flush to the top-left of its rect), and defect 4's fix
moved it to 92.3 / 21.4mm against WeasyPrint's 92.3 / 21.5mm.

The repro's own note that `@top-left` "appeared to work" is the tell: `@top-left`'s correct
x *is* the left margin, so the old bug left it looking right while `@top-center` looked
broken. The same Left/Right pattern shows up in the `page-selector` VRT golden.

### What the comparison did expose

A real, separate defect the repro did not isolate: `position: running()` did not take the
element out of the flow, so the source rendered **in the body as well** and displaced
everything after it — body text at 35.3mm where WeasyPrint puts it at 29.2mm.

The `position: running()` → `display: none` rewrite is applied while building
`cleaned_css`, and an inline `<style>` block's text is never rewritten:
`extract_gcpm_from_inline_styles` reads the constructs out of the DOM and leaves the
stylesheet alone. Delivered through an `AssetBundle` the same CSS behaved correctly, which
is what isolated it. **A themed document ships its CSS in a `<style>` block, so this is the
path real documents take.**

Fixed by injecting the hide rule with `InjectCssPass` after `RunningElementPass` has
harvested the content — the same route `counter_css` and the static pseudo-content already
use for exactly this gap. Covered by `crates/fulgur/tests/running_element_flow.rs`, which
asserts the general property: an inline `<style>` and an `AssetBundle` carrying the same
CSS must lay out identically.

**Three snapshots and one link test had baked the defect in**, asserting the duplicate as
correct output — `link_integration` literally expected "both source running element and
margin-box render" to carry a link annotation. All four were corrected against the
reference: WeasyPrint emits exactly one `/Link` here, and on `element(title, last)` it now
agrees with fulgur page-for-page (Override / Override / Two), which it did not before —
the running elements had been occupying space and mis-paginating the document.

## 3 — fixed

**Not WASM-only.** It reproduces on any host with `system_fonts(false)`, which is what the
tests use — WASM is simply always in that state, having no system fonts at all. That
reframing is what made it testable without a Worker.

The registered fonts are now installed as the collection's generic families *and* as its
Latin script fallback, so a family the document names but nobody registered still lands on
a real font (`blitz_adapter::install_last_resort_families`). Both hooks are needed:
generic families catch `font-family: Georgia, serif`, script fallbacks catch a bare
`font-family: Georgia` that names no generic for the mapping to reach.

It only applies when the collection would otherwise have **no** fallback at all, and that
is decided by asking the collection rather than by trusting the `system_fonts` flag — the
flag says what was requested, and on WASM it defaults to `true` while still yielding
nothing. A desktop caller who registers one font therefore still gets the host's `serif`
for `serif`, unchanged.

Covered by `crates/fulgur/tests/font_fallback.rs`: four tests that fail without the fix,
plus two controls that must not regress. The sharpest is pagination — unrendered text
occupies no space, so the reported 40-paragraph document collapsed to a single page. It
now lays out identically to the same document naming a registered family.

**One narrower case is still silent**: *no* font registered and no system fonts either.
Nothing can be fallen back to there, so it needs an error rather than a fallback — and
erroring at parse time would be wrong for a document that draws only shapes and has no
text to render. Distinguishing the two means noticing that text was laid out but shaped
to zero glyphs, which is a separate change. It matters for paperworx: a theme whose
`assets[]` carries no font at all lands exactly here.

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
