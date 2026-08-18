# paperworx repros

Defects found while evaluating fulgur as a PDF engine for
[paperworx](https://github.com/StudioMaak/paperworx), 2026-08-16 onward. Each was measured
against WeasyPrint 69 and Chrome 151 (headless-shell) on identical input; both of those
agree with each other, so they are the reference.

Intent: fix here, then offer upstream as separate PRs.

| # | defect | file | severity | status |
|---|---|---|---|---|
| 1 | Running elements not repeated per page | `01-running-element-per-page.html` | blocks NBB | **misdiagnosed** — see below |
| 2 | `<thead>` / `<tfoot>` not repeated on continuation pages | `02-thead-repeat.html` | blocks NBB | **fixed** — was a v2 regression, not a missing feature |
| 3 | Silent blank text when no registered font matches | `03-font-miss-silent-blank.md` | **dangerous** | **fixed** |
| 4 | Margin-box slots not anchored to their named position | `04-margin-box-anchor.html` | blocks NBB | **fixed** |
| 5 | Margin-box background does not fill the box's band | — | cosmetic | **fixed** (one part deferred) |
| 6 | `<tfoot>` before `<tbody>` renders at the top of the table | `scripts/refdiff/fixtures/table-tfoot-source-order.html` | real, single-page | **fixed** |
| 7 | Page 0 over-filled by the body offset; content spills past the page bottom | `scripts/refdiff/fixtures/break-inside-avoid.html` | real | **fixed** |
| 8 | `@font-face` with a `data:` URI is ignored; text silently falls back to a system font | `08-fontface-data-uri.html` | **dangerous** | **open** |
| 9 | A GCPM construct on anything but a bare tag/class/id selector is dropped silently | `09-gcpm-compound-selector.html` | **blocks NBB** | **fixed** |
| 10 | `vertical-align` is inert on an inline-block | `10-vertical-align-inline-block.html` | real | **open** |
| 11 | `margin-top` after a *forced* break is truncated to zero | `11-margin-after-forced-break.html` | real | **open** |
| 12 | `position: absolute` is ignored once the element is not a direct child of `<body>` | `12-absolute-bottom-nested.html` | real | **open** |

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

## 2 — fixed; it was a regression, not a missing feature

Covers both `<thead>` and `<tfoot>` repetition. Footer repetition is the mirror of
the header's, not a copy: a repeated header offsets where each page's strip
*starts*, a repeated footer shrinks where it *ends*.

**The earlier framing here was wrong, and the repo's own git history disproves it.**
fulgur *did* implement `<thead>` repetition — `TablePageable`, shipped in PR #14
(`065def71`, March 2026) — and lost it in the Phase 4 migration from the `Pageable`
tree to geometry-driven `Drawables` (`cd740c3d`, "delete pageable.rs, all Pageable
types removed"). The comments in `drawables.rs` ("not modelled in PR 5") and
`render.rs` ("deferred to a later change") were the v2 authors recording that they
had dropped it, not that it had never existed. `examples/table-header/` is v1's own
example, and the `is_header` flag in `convert::table::collect_table_cells` is what
survives of v1's classification.

Offer this upstream as *restoring* behaviour lost in the Drawables migration.

### The fixture was broken

`02-thead-repeat.html` had lost its `<html><head><style>` opening tags — the file
went straight from the HTML comment to bare CSS text and a stray `</style>`, so none
of the CSS applied. That is why WeasyPrint measured 8 pages here rather than the 7
recorded in this table. Repaired as part of this change; both engines then reproduce
the documented numbers exactly.

### Measured after the fix

| engine | pages | pages with header | unique rows | duplicated |
|---|---|---|---|---|
| WeasyPrint 69 | 7 | **7/7** | 300/300 | 0 |
| fulgur (before) | 7 | 1/7 | 300/300 | 0 |
| fulgur (after) | 7 | **7/7** | 300/300 | 0 |

Header bottom lands within **0.03pt** of WeasyPrint's on every page — the same order
as the font-metric residual seen in defect 4. Verified on three shapes: the fixture,
a table with no CSS at all, and a table starting mid-page after a heading.

### How it works

The header must *reserve* space, so it cannot be a post-pass like
`append_position_fixed_fragments` — `position: fixed` is out of flow and displaces
nothing, whereas header fragments emitted without reservation land on top of the
first row of every continuation page. `thead_repeat.rs` pins that: disabling the
reservation while keeping the emission puts the header's bottom *below* the topmost
row.

Blitz lays a `<table>` out as a flat cell grid — the table's `layout_children` are
the `<th>` / `<td>` boxes, and `<thead>` / `<tbody>` / `<tr>` carry no layout at all
— so a header cell is found by DOM ancestry, not by walking a `<thead>` layout child.

`fragment_block_subtree` computes the band once per table and reserves it at the
strip-overflow page cut, emitting `is_repeat = true` geometry for the header cells so
the existing per-NodeId dispatch redraws them whole on each page. Whether to reserve
is decided **per page**, following WeasyPrint's
`layout/table.py::all_groups_layout`: a header that would leave no room for a row is
dropped for that page. That makes the pathological cases — a header taller than the
page, or one that starves the first row — fall out instead of needing their own
guards, and guarantees the fragmenter still makes progress.

Only the header carries `is_repeat`. v1 shipped a bug where table *body* cells were
cloned wholesale onto every page instead of sliced per page (fixed in `4d44c483`);
widening `is_repeat` past the header would reintroduce exactly that.

### `<tfoot>` measured

Same 300-row fixture with a `<tfoot>` after `<tbody>`:

| engine | pages | header | footer | rows | dup | footer overlaps |
|---|---|---|---|---|---|---|
| WeasyPrint 69 | 7 | 7/7 | **7/7** | 300/300 | 0 | 0 |
| fulgur (before) | 7 | 7/7 | 1/7 | 300/300 | 0 | 0 |
| fulgur (after) | 7 | 7/7 | **7/7** | 300/300 | 0 | 0 |

Both engines place the footer immediately after the last row on each page rather
than flush to the page bottom — WeasyPrint via
`footer.translate(dy=end_position_y - footer.position_y)`. fulgur holds a constant
**+6.00pt** gap from the last row on every page including the last, where the table
ends mid-page; WeasyPrint holds **+4.50pt**. The 1.5pt difference is row
padding / line-height, not a placement error.

Absolute footer positions diverge on the final page (261pt vs 635pt) purely because
of row packing: WeasyPrint fits 48 rows per page against fulgur's 44, so 13 rows land
on page 7 rather than 36. Both total 300. That difference predates this change.

### Known limitations

- **Collapsed borders are not resolved for the repeated header.** With
  `border-collapse: collapse`, WeasyPrint tracks `body_rows_offset = skipped_rows -
  header_rows` in `draw/__init__.py` purely to resolve a repeated header row's
  borders against the first body row. fulgur does not.
- **A cell whose own content splits across pages does not carry the header** on the
  pages that split spans. The recursion places that content from y = 0 on the new
  page, so there is no reserved room there; the header is omitted rather than
  overlapped.
- **Forced breaks inside a table do not repeat either band** — measured: `break-before:
  page` on a `<td>` is not honoured by fulgur at all, so those page-advance paths are
  unreachable for tables today and deliberately carry no reservation logic.
- ~~**A `<tfoot>` written before `<tbody>` is not repeated at all**~~ — fixed by
  defect 6. The section is put into CSS box order before layout, so the shape that
  reaches the fragmenter is the ordinary one and the footer repeats like any other.
  `table_section_band`'s edge check still bails on a band that is genuinely
  mid-table — see item 6.

## 6 — fixed; `<tfoot>` before `<tbody>` rendered at the top of the table

Found while measuring defect 2's footer half. **Not a pagination defect** — it needs no
multi-page document, and it lived in table layout rather than in the fragmenter.

fulgur laid table sections out in source order. CSS 2.1 §17.5.1 puts a table's boxes in
header group → row groups → footer group order regardless of the source, and HTML4
*required* authors to write `<tfoot>` before `<tbody>`, so real documents hit this.

Single page, 40 rows, `yMin` top-down (bigger = lower on the page):

| fixture | engine | header | rows | footer | verdict |
|---|---|---|---|---|---|
| `<tfoot>` before `<tbody>` | WeasyPrint 69 | 64.7 | 79-643 | **657.8** | below rows |
| `<tfoot>` before `<tbody>` | fulgur (before) | 65.7 | 96-703 | **80.7** | **above rows** |
| `<tfoot>` before `<tbody>` | fulgur (after) | 65.7 | 80-687 | **703.2** | below rows |
| `<tfoot>` after `<tbody>` | WeasyPrint 69 | 64.7 | 79-643 | 657.8 | below rows |
| `<tfoot>` after `<tbody>` | fulgur | 65.7 | 80-687 | 703.2 | below rows |

WeasyPrint's output is identical for both orderings, as it should be, and fulgur's now
is too. The residual gap to WeasyPrint (703.2 against 657.8) is row packing, unchanged
by this work and present in the already-correct ordering before it — the fix makes the
misordered document render *exactly* what the well-ordered one renders, which is the
property that matters.

### It was not only `<tfoot>`

`<thead>` written *after* `<tbody>` — which the HTML parser also accepts — was wrong the
same way and by the same root cause: fulgur drew the header at `yMin` 687.4, below every
row, where both references keep it at 64.7. Fixing only the footer would have left the
defect reachable and made "source order does not matter" false as a property. Measured
on nine one-variable probes, fulgur now reproduces WeasyPrint's vertical ordering on
every one.

### Root cause

Not fulgur's own layout: blitz-dom builds a table's cell grid by walking the table's DOM
children in order (`layout/table.rs::collect_table_cells`), giving `TableHeaderGroup`
and `TableFooterGroup` the same arm as `TableRowGroup` and assigning grid rows as it
goes. Nothing downstream ever reordered them.

### The fix

`blitz_adapter::TableSectionOrderPass`, a `DomPass` registered in `engine.rs` right
after `CaptionRestructurePass` and following its shape: it runs before the engine's own
`resolve()` and re-resolves internally, because the classification is a *computed*
display. A tag-name prefilter finds tables whose `<thead>` / `<tfoot>` are out of place
before any restyle, so a document with no misordered table never pays that resolve —
and by exhaustive scan, the only misordered table in this repo is the refdiff fixture.

Two details are load-bearing:

- **Keyed on computed display, not on the tag name.** `<tfoot style="display:
  table-row-group">` is an ordinary row group that both references leave in source
  order (WeasyPrint draws it at `yMin` 79.2, above the first row at 93.6; Chrome 151
  agrees). A tag-name test would wrongly demote it. This is also the predicate
  blitz-dom itself dispatches on.
- **Only the *first* header group and the *first* footer group are promoted**; every
  other child, additional header / footer groups included, keeps its source position.
  That is CSS 2.1's model, and both references reproduce it exactly: on
  `<tfoot A><thead A><tbody><tfoot B><thead B>` they both draw
  `HEAD A → rows → FOOT B → HEAD B → FOOT A`.

The `CaptionRestructurePass` hazards were checked and do not apply the same way. Blitz
dropping a non-table-typed child of `<table>` is not reachable here — a section stays
inside the same table rather than being lifted out — and for the same reason no
`visibility: hidden` guard is needed, since no cascade that hid the section can be
escaped by the move. A `display: none` child is never classified as a section group at
all (its `DisplayInside` is `None`), and a `display: none` table is skipped outright.

One Blitz sharp edge did bite: `Mutator::append_children` cannot move a node within its
*own* parent. It appends the id, then runs `old_parent.children.retain(|id| *id !=
child_id)` on what is the same node, stripping the copy it just appended and dropping
the child out of the tree. The move goes through `remove_node` first, which leaves the
node in the slab (only `remove_and_drop_node` frees it) and keeps the restyle / damage
marking intact.

### Defect 2's footer repetition now handles this shape — deliberately

`table_section_band_bails_on_a_tfoot_that_is_not_the_bottom_band` was planted to fail
when this was fixed, and it asserts its own premise so it could not pass for the wrong
reason. It needed a decision rather than a patch, and the decision is **yes**: once the
footer genuinely *is* the table's bottom band, repeating it is what WeasyPrint does —
its two orderings are identical, footer on 7/7 pages either way. So the misordered
fixture is now repeated too, asserted end-to-end by
`a_tfoot_written_before_tbody_repeats_on_every_page_its_table_spans`.

The test itself was rewritten rather than deleted, because the guard it covers is still
load-bearing — just for a different shape. The pass keys on computed display while
`is_in_table_section` keys on the tag name, so a `display: table-row-group` `<tfoot>`
stays mid-table by design and its cells must not be repeated from there. That is the
bail case now, and it still asserts its own premise.

**It would not have gone red on its own.** The `parse` helper in `pagination_layout.rs`
runs no DOM passes, so a pass-level fix leaves it green while it quietly asserts a tree
the engine never produces — worse than a failure, since nothing would have prompted the
decision. It now goes through `parse_with_section_order_pass`, and both it and the
rewritten positive case were verified to fail with the pass disabled.

### Structural selectors still match source order

A browser reorders *boxes* and leaves the DOM alone, so `:nth-child` keeps counting
source positions. Moving DOM nodes through `DocumentMutator` does not: it marks the
parent `RestyleHint::restyle_subtree()`, so the next resolve re-matches against the
moved order. Measured on `table > tbody:nth-child(3) td {padding-left: 40pt}` with
`<tfoot>` written first — the rule applies in WeasyPrint (first cell at `xMin` 102.7)
and, in the first cut of this fix, no longer did in fulgur (65.7).

Chrome 151 says the same thing directly rather than by inference: the `<table>`'s DOM
children stay `[thead, tfoot, tbody]`, `tbody.matches(':nth-child(3)')` is true and
`:nth-child(2)` false, the computed `padding-left` is 53.33px = 40pt — and the footer
is still drawn below the last row (top 156.7 against 135.4). Boxes move, the DOM does
not. All three engines agree on the shift the rule produces: +37pt, replacing a 3pt
padding with a 40pt one.

Note that Chrome's `getBoundingClientRect()` on the `<td>` reports `left: 8` either
way — padding is inside the cell, so only its text moves. Measuring the element box
makes the rule look inert; the text extent (`pdftotext -bbox`, or a `Range` over the
cell contents) is what to probe.

Fixed by separating the two things the mutator conflates. The pass resolves the cascade
first, then reorders by rewriting `Node::children` directly — no restyle marking — so
box construction walks the new order while the styles keep the ones matched against the
old one. fulgur now puts that cell at 102.4, the usual font-metric residual from
WeasyPrint's 102.7, with the footer still at the bottom.

Two consequences worth knowing:

- **The pass runs last**, immediately before the engine's `resolve()`. Anything that
  injects CSS after it — the GCPM hide rule, `counter_css`, static pseudo content —
  would dirty the stylist and re-cascade against the moved order. Its own internal
  resolve flushes those stylesheets *before* the reorder, leaving the engine's resolve
  with nothing to restyle. Pinned by
  `injected_css_does_not_re_cascade_against_the_moved_order`, measured on a document
  carrying counters, `::before` content and a margin box at once.
- **It depends on blitz-dom rebuilding the box tree on every resolve**, which
  `resolve_layout_children` does under `NON_INCREMENTAL` — blitz-dom 0.2.4's default,
  since its `incremental` feature is off and fulgur does not enable it. The damage
  constants that would let the pass request a rebuild explicitly live in a private
  module and cannot be named from outside the crate. If that ever changes the reorder is
  simply ignored and the footer returns to the top, which the tests fail loudly on.

Running the pass last also means counters and bookmarks are harvested in source order,
which is what CSS specifies — they traverse the element tree, not the box tree.

Out of scope, and for the same reason as always — the references disagree with each
other, so there is no reference to build against: a `<div style="display:
table-footer-group">` child of a `<table>`. Chrome hoists it *above* the table,
WeasyPrint drops its text entirely. fulgur matches WeasyPrint here both before and
after this change.

### Verification

`scripts/refdiff` rated this fixture `MISSING` — the harness's strongest divergence
class, fulgur omitting the footer on 2 of 3 pages — and now rates it `PACKING`, the
font-metric class its two sibling table fixtures already sat in. That was the last
`MISSING` row in the harness.

Covered by `crates/fulgur/tests/table_section_order.rs` (10 tests, each verified to
fail against the build before it, bar the two controls that must not regress) and ten
unit tests on the pass itself. The sharpest is the property, not a coordinate:
`source_order_of_the_sections_does_not_change_the_output` renders both orderings and
requires every drawn run to match exactly — which is what WeasyPrint does, and the
thing the defect actually broke.

No golden or snapshot moved, and none could: a scan of every `.html` in the repo plus
every table built in Rust test sources found exactly one misordered table, the refdiff
fixture, which is not a VRT input. `fulgur-vrt` was therefore not regenerated — it
cannot run on macOS anyway.

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

## 5 — fixed; and it uncovered a silent content-loss bug

A margin box's background now fills its whole band, matching Chrome exactly
(**y 272.3-296.3mm**, against Chrome's 272.3-296.3). It used to paint only
282.9-285.8mm — the strip the text happened to occupy.

The author's declarations used to render on an inner wrapper, and a wrapper is sized to
its own content. They now go on a box `<div>` that takes the rect's height, inside a
`<body>` that stays the untouched slot. `margin` is re-zeroed after them — the box is
positioned by fulgur, not by the document — while `padding` stays overridable, since
insetting content within the background is what padding is for.

Chrome is the authority here because the two references disagree: Chrome's box *is* its
rect on both axes, WeasyPrint shrink-wraps horizontally. That disagreement also explains
the `text-align` divergence noted under item 4.

### The bug it uncovered

Building this exposed a **pre-existing silent content-loss bug at the fork point**:
`@left-bottom` and `@right-bottom` boxes drew **nothing at all** unless the at-rule
happened to carry extra declarations. A bottom-aligned box puts its content's bottom edge
exactly on its own height, which is also the render pass's page height — a float
comparison then tips it onto a second page that is never drawn. Every probe in the
original defect-4 work carried `font-size: 8pt`, whose extra nesting level dodged the
boundary, so it stayed hidden.

Half a CSS pixel of slack on that page height fixes it. It is far too small to change
where genuinely overflowing content is cut. Same class as item 3: content vanishing while
the render reports success.

### Still deferred: overflow containment

Content *taller* than its band now spills out of it instead of being cut off, because the
box has an explicit height and centring overflows in both directions. The references
disagree again — WeasyPrint spills the same way, Chrome contains it — so this is a
behaviour change, not a regression against the reference, but it does differ from what
fulgur did before.

`overflow: hidden` does not clip in the margin-box render path, so containing it needs the
clip machinery the main render path uses (`clip_descendants` / `draw_under_clip_table`).
That is the remaining half of this item.

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

## 8 — `@font-face` with a `data:` URI is ignored

Found while measuring fulgur against production on a real NBB filing, and it invalidated
the first run of that measurement.

A theme delivers its fonts as `@font-face` rules with base64 `data:` URIs — that is how the
rendered HTML is self-contained. fulgur ignores them and falls back to a system font,
reporting success. Chrome and WeasyPrint both honour the rule.

```bash
fulgur render -o out.pdf 08-fontface-data-uri.html
mutool clean -d out.pdf - | grep -o "/BaseFont */[A-Za-z0-9+-]*" | sort -u
```

| engine | embedded font |
|---|---|
| WeasyPrint 69 | `ZRPRAM+Probe-Sans` |
| fulgur | `XUZQRR+Helvetica` |

Same on the real filing: Chrome embedded `LiberationSans`, fulgur embedded only
`Helvetica`. Passing `--font` / `AssetBundle` is the workaround, and the integration plan
already required it — but nothing says so at the point of failure, and the document still
renders, so the substitution is invisible until you inspect the embedded fonts.

**This is the measurement trap of this whole exercise.** Every position taken against a
substituted font is measuring the wrong document. A first pass here reported a
`fi`-ligature corruption in the text layer (`financiële` extracting as `fnanciële`, x35)
as a separate defect; it was entirely a symptom of this one. With the font registered,
fulgur extracts `financiële` x35 — identical to Chrome. There is no ligature defect.

Relation to item 3: that fix is what keeps this from being a blank page. The fallback is
working as designed; the gap is that a declared, *available* font is never loaded.

## How they were measured

```bash
# reference engines
uvx --from weasyprint weasyprint in.html out-weasy.pdf
# fulgur
cargo run -p fulgur-cli -- render -o out-ful.pdf in.html
# compare text positions in mm
pdftotext -bbox -f 1 -l 1 out.pdf -    # xMin/yMin are pt; * 25.4/72 = mm
```

## 7 — fixed; page 0 was over-filled by the body offset

Found by `scripts/refdiff`, not by hand — the first thing that harness caught
that nothing else had. **The name is a misnomer**: the fixture that surfaced it
uses `break-inside: avoid`, but the defect has nothing to do with it. The same
divergence reproduces with `break-inside: auto` and with empty blocks, which is
what killed the first hypothesis.

### Root cause

`render_v2` shifts page 0's fragments down by `body_offset_pt.1` — body's own
`location.y`, which absorbs the collapsed top margin of the first in-flow child
— and applies it to **page 0 only**; continuation pages are already
page-content-area-relative because the fragmenter resets `cursor_y` to 0
(`render.rs`: *"only on page 0 (continuation pages are already
page-content-area-relative after the fragmenter resets cursor_y)"*).

The fragmenter, however, compared body-relative cursors against the **full**
page height. So page 0's usable height was over-stated by exactly the body
offset, and whatever landed last on it spilled past the page bottom.

Measured on the fixture: page height 971.35px, body offset 15px, page 0's
content reaching 964px body-relative — true bottom 979px, a **5.74pt overflow**,
and 2 pages where WeasyPrint uses 3.

### It was not rare

A 100-paragraph document with fulgur's default 20mm margins (content band
56.69–785.20pt) drew **12 words below the content bottom**, worst overflow
8.74pt — into the band where `@bottom-center` renders the page number, so body
text and the page number overlap. After the fix: **0 words below the bottom**,
matching WeasyPrint exactly (4 pages, 0 overflow, both engines).

It only changes the *page count* when the last block on page 0 happens to land
within the offset window, which is why it stayed invisible: usually it just
prints a line or two into the bottom margin.

### The fix

Page 0's capacity is `page_height_px - body_offset_y`; every later page keeps
the full height. Applied at the four decision sites in
`fragment_pagination_root` — inline-root promotion, the recursion gate's
available strip, the block strip-overflow cut, and an oversized child's first
slice.

Covered by `crates/fulgur/tests/body_offset_capacity.rs`, whose fixture is
font-independent (every height an explicit `mm`) and places the page boundary
inside the offset window so the two capacity models give 3 pages versus 2. Its
control asserts that a document with **no** body offset keeps the full first
page — guarding against "fixing" this by shrinking every page, which would
under-fill documents that never had an offset.

`scripts/refdiff` now reports this fixture as `AGREE` at 3 pages against
WeasyPrint's 3, max Δy 1.3pt.

### Two goldens had encoded it

`gcpm_multipage_counter` and `gcpm_string_set_chapter_title` both changed. Their
committed snapshots placed a text baseline at **792.44pt** against a content
bottom of **785.20pt** — 7.24pt into the bottom margin. They were regenerated
only after measuring that the pre-fix build drew 12 words past the page bottom
and the post-fix build draws none.

That is the fourth golden in this fork found to have encoded a defect as
expected output. Regenerating is never verification; see
`docs/test-harnesses.md`.

### Still open

The same capacity model applies inside `fragment_block_subtree`, which receives
`page_height_px` and does its own overflow comparisons. No fixture reproduces a
failure there — a body-direct child would have to recurse on page 0 *and* have
its internal break land within the body offset of the bottom — so it is
deliberately left alone rather than changed speculatively without a failing
test.

## 9 — fixed; and the "cumulative vertical drift" was four defects, none of them cumulative

Found while chasing the vertical disagreement between fulgur and production on a real
NBB filing (VOL-kap, 66 pages, A4). The disagreement is real and it does grow down the
page, but **it does not accumulate** — it is four independent defects whose errors happen
to be positive in the upper half of page 1. Only this item is fixed here; 10, 11 and 12
are each isolated to a one-variable repro and left open.

### The measurement was reading the wrong number

`pdftotext -bbox`'s `yMin` is *baseline − ascent*, and the two engines declare different
ascents for the same font: Chrome writes hhea (Liberation Sans `1854/2048` = 0.905em),
krilla writes OS/2 typo (`1491/2048` = 0.728em). At 9pt that is a **constant +0.56mm** on
every fulgur `yMin`, present even where the two engines agree exactly.

The originally reported page-1 progression — +0.50 / +0.88 / +2.33 / **+3.91mm** — is that
constant plus the real error. Comparing true baselines instead (`mutool draw -F stext`
reports the glyph origin and the font size directly) gives:

| word y (Chrome) | reported Δ | true baseline Δ |
|---|---|---|
| 20.5mm | +0.50 | −0.09 |
| 31.1mm | +0.88 | +0.18 |
| 133.7mm | +2.33 | +1.76 |
| 190.0mm | +3.91 | +3.35 |

More usefully, the true-baseline series **is not monotonic**. Down page 1 it runs
+0.18 → +0.18 → **+2.56** → +0.18 → **+2.82** → +0.97 → **+3.35** → +0.71. It rises and
returns. Nothing is accumulating; specific *structures* are misplaced and the flow
re-syncs after each one. That is what killed the line-pitch hypothesis, and it is why
zipping word lists produced a plausible-looking ramp: it averages localized errors of
different sizes into a trend.

**Use `mutool draw -F stext` for vertical comparisons, not `pdftotext -bbox`.** It reports
the baseline (`y=`) and the font size, so nothing depends on what either writer put in the
font descriptor. `pdftotext -bbox` stays fine for horizontal work, which is why the
0.02–0.07mm horizontal agreement held up.

### Root cause of item 9

`GcpmSheetParser::parse_prelude` accepted exactly one bare simple selector — a tag, a
class, or an id — and returned `Ok(None)` for everything else, which makes `parse_block`
skip the rule without recording a mapping or a CSS edit. Silently.

The filing declares its running header the way a themed document does:

```css
section[data-section="nbb-page-header"] { position: running(nbbrh); padding-bottom: 7.5pt }
```

That is an attribute selector, so `position: running(nbbrh)` was never seen. The
consequences compound:

- `@top-center { content: element(nbbrh) }` had nothing to draw, on any page.
- The 52 source `<section>`s were never taken out of the flow. Each is 23.1pt tall
  (`15.6pt` inline-block + `7.5pt` padding), so every one of them printed in the body and
  pushed the rest of its page down by **8.11mm** — measured on page 2, against a header
  height of 8.15mm.

`.rh { position: running(rh) }` — the shape every fixture in this repo and every earlier
repro used — worked perfectly, which is exactly why this survived to a real document.

### Measured on the filing

Both references were re-measured on the same generated HTML. WeasyPrint 69 agrees with
Chrome + Paged.js to **0.2mm** on page-2 body text, so on this document the polyfill's
body placement is not in question and the two references are as usual jointly the
reference.

| build | pages | pages carrying the running header | of which inside the top margin band |
|---|---|---|---|
| Chrome 151 + Paged.js | 66 | 65 | **65** |
| WeasyPrint 69 | 68 | 68 | **68** |
| fulgur before | 67 | 52 | **0** |
| fulgur after | 70 | 69 | **69** |

Before the fix the header appeared 52 times — once per source section, in the body,
never in a margin band. After it, 69 of 70 pages carry it in the band, which is the shape
both references produce.

Page-2 body text moves from **+8.11mm** against Chrome to **−3.0mm**, and the residual
is item 11 (`section[data-section="nbb-sectie-2-1"] section[data-section="nbb-titelbox"]
{ margin-top: 8.6pt }` = 3.03mm, truncated at the forced break above it).

**The page count got further from Chrome, and that is the fix working.** 67 was a
coincidence: roughly 1.5 pages of running-header sections wrongly in the flow were
cancelling pages fulgur was under-producing elsewhere. Removing the wrong content leaves
70 against WeasyPrint's 68 — a real 2-page residual that items 10–12 have to account for
before the count can be trusted.

### The fix

`ParsedSelector` gains a `Compound` variant: an optional type selector plus any number of
`.class` / `#id` / `[attr]` qualifiers on the same element. That is the whole of what a
selector can say about one element without a combinator, so it is the natural stopping
point — and it needs no tree walk, which is what `selector_matches` and the injected
`display:none` rule both rely on.

Three details are load-bearing:

- **The bare forms still reduce to `Tag` / `Class` / `Id`.** A `.foo` arriving as a
  one-part `Compound` would still match, but every existing assertion on
  `ParsedSelector::Class` would break and the injected CSS would gain needless nesting.
  Pinned by `test_bare_selectors_still_reduce_to_simple_variants`.
- **An attribute selector carrying a case-sensitivity flag (`[a=b i]`) is rejected, not
  matched case-sensitively.** The flag changes *which* elements match, and this selector
  is re-emitted as a `display:none` rule — matching more elements than the author wrote
  would hide the wrong ones. Same reasoning for a namespace.
- **Unsupported preludes are no longer silent.** Combinators and selector lists still
  cannot be represented; when the block they introduce carries a paged-media-only
  declaration (`position: running()`, `string-set`, `bookmark-*`) the parser now
  `log::warn!`s with the offending selector text. `content` and `counter-*` are
  deliberately excluded — they are ordinary CSS and would fire on every stylesheet.

`selector_to_css` replaces the two hand-written match arms that rebuilt a selector for
`build_running_hide_css` and `build_static_content_css`, so the injected rule and the
matcher cannot drift apart.

### A unit test had encoded the limitation

`test_compound_selector_block_is_drained` asserted that `.foo.bar { position:
running(header) }` registers *nothing*. That was the defect stated as expected behaviour —
the fifth such case in this fork. It was not simply regenerated: `.foo.bar` was rendered
through WeasyPrint 69 first, which puts the running element in the margin box and takes it
out of the flow, and fulgur now matches it (RUNHEAD at 10.85mm against 11.40mm, the usual
font-metric residual). The test was then rewritten as
`test_unsupported_selector_block_is_drained` over a descendant combinator, which is
genuinely still unsupported, so the drain-and-continue behaviour it covers stays pinned —
plus `test_selector_list_block_is_drained` for the other unsupported shape.

### Verification

`crates/fulgur/tests/gcpm_compound_selector.rs` — four tests, three of which were verified
to fail against the build before the fix. The sharpest is the property rather than a
coordinate: `the_selector_shape_does_not_change_the_layout` renders the same document
through six selector shapes and requires byte-identical page counts, margin-band counts
and body y-positions against the `.hdr` control. That is what WeasyPrint does, and the
thing the defect actually broke.

Plus ten unit tests on the parser covering every attribute operator, the case-flag
rejection, the compound reduction rule, and both unsupported shapes.

**No golden moved, and that was checked rather than assumed.** Every GCPM construct in
this repo — eight VRT fixtures, two examples, the refdiff fixture, repro 1 — is declared
on a bare `.class`, `h1`, `h2` or `h3`, all of which still reduce to the same simple
variant. `fulgur-vrt` was run in `ubuntu:24.04` with `fonts-dejavu-core` 2.37-8 and passes
byte-exact.

## 10, 11, 12 — isolated, not fixed

These three are what remains of the filing's vertical disagreement. Each has a
one-variable repro measured against WeasyPrint 69; none is a pagination defect, and none
of them is cumulative.

**10 — `vertical-align` is inert on an inline-block.** fulgur produces byte-identical
output for `middle`, `baseline` and `top`, so no value of the property does anything. The
error grows with the box: 0.00mm at a 10pt box, +3.67mm at a 30pt one. This is what page 1
of the filing measures — its form rows put 15.7–18.4pt bordered inline-blocks on
`vertical-align: middle`, and they sit 1.8–3.4mm low. It is the one that fails the ±1%
gate on page 1 by itself (2.97mm), and fixing item 9 does not touch it.

Note fulgur does not agree with WeasyPrint's `baseline` either, so "implement the missing
values" is likely not the whole story — the line box's own height model wants measuring
first. CLAUDE.md already records that `display:table-cell` + `vertical-align:middle`
silently does nothing, which is plausibly the same gap seen from the other side.

**11 — `margin-top` after a forced break is truncated to zero.** css-break-3 §5.4
truncates margins adjoining an *unforced* break; at a forced break they are retained, and
both references retain them. Worth **−3.0mm** at every section opening in the filing, and
the repro is font-independent (every length an explicit `pt`).

**12 — `position: absolute` is ignored once the element is not a direct child of
`<body>`.** One unstyled wrapping `<section>` is the whole difference between fulgur
pinning the block correctly (282.22mm) and dropping it at its flow position (71.35mm).
A static ancestor is not a containing block, so the wrapper cannot matter — WeasyPrint
puts it in the same place either way. In the filing this puts the page-1 footnotes 28mm
high, overlapping the signature table.

### What is *not* a defect here

The running **header**'s own placement. fulgur puts it at 19.05mm on page 2 against
WeasyPrint's 17.74 and Paged.js's 17.46. That is the margin-box vertical box model
recorded in CLAUDE.md, already verified against native Chrome and WeasyPrint across all
sixteen slots, and it is a migration consideration rather than part of this work.

The footer margin box is likewise fine — 284.27mm against Chrome's 283.99/283.73.

### Reproducing the filing

The corpus is paperworx template XML, not HTML; generate through paperworx's own harness.

```bash
cd .../paperworx/templates/nbb-jaarrekening/tools
npx tsx local-render.mts /tmp/vol-kap.html vol-kap
node printpdf.mjs /tmp/vol-kap.html /tmp/vol-kap-chrome.pdf
```

`local-render.mts` wants Liberation Sans at `/tmp/liberation-fonts-ttf-2.1.5/`. **Pass
those fonts to fulgur explicitly** — without `-f` it ignores the HTML's `@font-face`
`data:` URI and substitutes Helvetica (item 8), which invalidates every position measured.
Confirm before trusting a number:

```bash
mutool clean -d out.pdf /tmp/c.pdf && grep -a -o "/BaseFont */[A-Za-z0-9+-]*" /tmp/c.pdf | sort -u
```

Both engines legitimately embed Helvetica *as well as* LiberationSans on this document, so
the check is that LiberationSans is present, not that Helvetica is absent.
