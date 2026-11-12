# Pagination walker: place boxes, stop re-anchoring

Status: proposal, fork-only (no upstream PRs for now). 2026-11-11.

## Why

Four bugs in a row came from one mechanism, and each fix was a local rule guessed from the shape that broke:

| # | Shape | Symptom | Fix so far |
|---|---|---|---|
| 1 | forced break, then a `display:none` / running sibling, then content (vol-kap) | content placed at its unbroken offset: pushed a page on, or painted below the page | defer the re-anchor past zero-height children (`16451d6d`) |
| 2 | forced break, then a real zero-height box with `margin-bottom` | the margin is dropped (39.6pt vs Prince 61.2pt) | defer only past `display:none` (fix3, on fork PR #6) |
| 3 | zero-height forced break inside a flex column or grid | next item drawn on top of earlier content on the same page | fold the break's page advance into `RowState.max_end_*` (fix4, on fork PR #6) |
| 4 | `margin-bottom` on a zero-height box after a forced break, at body level | margin dropped | none; pre-existing in every version |

The cause is the nested walker's model, not any one of these rules. `fragment_block_subtree_inner` places a child at `page_start_y + (location.y - page_taffy_origin)`. That is, it reconstructs page positions from one Taffy offset that must be re-picked after every break. It is re-picked from "whichever child comes next", deferred through `origin_pending_target_y` and `origin_pending_same_row`. `page_taffy_origin` has 7 assignment sites, the two pending fields 12, and `page_start_y` 14. Every special case (box-less child, zero-height box, flex row, inline split, recursion crossing a page) shifts the offset for everything after it.

The body walker (`fragment_pagination_root`) does not have this problem. It already places each child at `cursor_y + gap`, with `gap = location.y - prev_bottom_y_in_body`. It knows the previous box, not an origin.

## Evidence: the break matrix

210 synthetic cases, plus 42 tall-content variants, each rendered with fulgur base / fix2only / fix3 and with Prince 17. Chrome 157 is the second reference where Prince ignores breaks (flex/grid).

- **Axis A:** what follows a zero-height `break-after:page`. One of: a control (nothing), `display:none`, a running element, `display:contents`, abs/fixed, float, a comment, an empty span, `visibility:hidden`, two zero-height boxes, border-top, padding, `margin-top`, `margin-bottom`, or minified markup.
- **Axis B:** where it sits. One of: body level, in a `<section>`, two deep, a flex column, or a grid.

| version | matches Prince (all 210) | misses outside flex/grid (126) |
|---|---|---|
| base `cd9d9583` | 94 | 32 |
| fix2only (`16451d6d`) | 102 | 24 |
| fix3 (display:none only, `c29d4d0d`, PR #6) | 114 | 12 |

fix3's remaining misses outside flex/grid are bug 4 (6 cases) and floats (6 cases). The float misses are a layout issue, not a pagination one: ZZZ goes below a float instead of beside it, even with no break at all. No version painted text past the content box in any case.

The script lives in the session scratchpad (`matrix/run.py`). Step 0 turns it into a checked-in test.

## Target model

The walker holds the boxes it places, instead of an origin it re-derives.

1. **One pre-pass per container builds a flow list.** Each entry is either:
   - `Box { node, top, height, gap_before, margin_top, breaks, used_page, row }`, where `gap_before` is the Taffy distance from the previous *box's* bottom (the collapsed margin or the flex/grid gap), and `row` groups flex/grid items that share a line;
   - `Marker { node }`, for anything without a box that still has to be recorded: `display:none`, running elements, and zero-area nodes carrying counters, string-set or bookmarks.

   Whitespace text and abs/fixed boxes are dropped here, once. Today the same filter is written out four times: in the body walker, the nested walker, `would_split_block_subtree` and `has_page_name_change_below`.
2. **Placement is local arithmetic.** A box goes at `prev_end + gap_before`. After a forced break it goes at the page top plus the margin rule, applied explicitly (the fork keeps the box's own margin, `874877ef` / `8d75e147`). After an unforced break the margin is truncated. A marker records a zero-height fragment at the current cursor and never moves anything.
3. **Rows are placed as units.** A row starts at the previous row's max end plus the gap. Each cell starts at `row_y + (cell.top - row.top)`. A cell that crosses a page carries the row's start page and cursor, as `RowState` does today, but the row is known from the pre-pass rather than detected on the fly with `location.y < row_bottom - 0.5`.

Bugs 1-4 cannot be written in this model. A box-less node is never "the previous box"; a real zero-height box is; and a break inside a row is a row event, not an origin shift.

## Steps

Each step lands on its own and keeps the suite green. Each one also reports its blast radius: CLI render of every VRT manifest fixture and every example, byte-diffed against the previous step.

0. **A checked-in break matrix.** Move the matrix into `crates/fulgur/tests/break_matrix.rs`. The expected page and y come from the reference engines and are written down as literals, never recomputed. Cases that fail today are marked `KnownBug("<label>")`. A known bug that starts matching fails the test until its row is promoted to `Pass`, so every later step has to record what it fixes.
   - Where the references disagree (decided 2026-11-11):
     - Flex/grid forced breaks: follow Chrome and honour the break. Prince ignores it.
     - A zero-height box's `margin-top` after a forced break: follow Chrome and keep it. Prince drops it but keeps `margin-bottom`, which is inconsistent for a margin that collapses through.
     - Running elements: Chrome does not implement them, so expect the same position as the `display:none` case.
   - Use the same `pdftotext` helper as `content_past_page_bottom.rs`: it skips locally when pdftotext is missing and fails under `CI`.
1. **The flow-list pre-pass, with no behaviour change.** Add `flow_items(doc, parent) -> Vec<FlowItem>`. Have the body walker, the nested walker, `would_split_block_subtree` and `has_page_name_change_below` iterate it instead of the raw children.
   - Gate: 0 fixtures moved, suite unchanged, matrix unchanged.
2. **The nested walker places by previous box.** Replace `page_taffy_origin`, `origin_pending_target_y` and `origin_pending_same_row` with `prev_end` + `gap_before`, matching the body walker. Rebuild `RowState` on the pre-computed rows.
   - Expected wins: bugs 1-3 and the flex/grid overlap cases.
   - Folds in the held-back fix on local branch `wip/first-child-occupied`: a container's first child never moves to the next page, so a table's first row in the last few px of a page is drawn past the bottom. In this model "something is already on this page" is just `prev_end > page top`. Add the reviewer's repros as matrix cases before touching it. The branch's naive flag pushed a short stretched flex/grid cell a page on (81pt gap), and gave a blank first page when only padding is above a container. Its four rewritten `*_cosplit_*` tests lost their fulgur-ysms guard; don't reuse them.
   - Gate: matrix promotes those cases to `Pass`, suite green, and every moved fixture explained.
   - **Done 2026-11-12** on `paperworx/walker-place-by-prev`; see *Step 2 as built* below.
3. **Body-level margin after a zero-height break (bug 4).** Small once step 1 exposes `margin_top` and `gap_before` on the box that actually follows the break.
4. **Optional: one walker.** Merge body and nested into one function parameterised by `capacity(page)`. This removes the gate drift the map found:
   - multicol with `column-span:all` recurses only at body level;
   - transformed inline roots split only at body level;
   - nested walkers ignore the page-0 body offset.

   It also makes `would_split_block_subtree` a dry run of the real placement instead of a second copy of the gap model.

## What must not change (output contract)

From the walker map. Any step that breaks one of these is wrong, whatever the matrix says.

- **Order.** `fragments.first()` is the earliest page. `collect_string_set_states`, `collect_running_element_states`, `collect_counter_states`, anchors, bookmarks and `paragraph_lines_for_page` rely on it.
- **Split heights.** For split nodes, the sum of the earlier fragments' heights is the consumed offset. Paragraph line slicing and multicol slices rely on it, so inline-root fragment heights must equal real line extents.
- **Coverage.** Every in-flow node, zero-height and running ones included, and every container has at least one fragment. `implied_page_count` is the max `page_index + 1`.
- **`is_repeat`.** Only the fixed-element and table-band repeat paths set it.
- **Determinism.** Same input, same bytes.

The table-repeat path (`fragment_repeating_table`) feeds `_inner` cell-level children with `initial_taffy_origin = body_origin_px`, and shifts body geometry by the band height afterwards. In the new model that becomes an initial `prev_end`, so it has to move with step 2.

## Risks and costs

- **Size.** `_inner` is 1103 lines, the body walker 801. The test module has 163 tests, 106 of which call a walker and many of which assert exact coordinates. Some of those assertions encode today's behaviour, not the spec's. Each changed assertion needs a reference measurement, not a rewrite to match the new output; that was the trap with the withdrawn `*_cosplit_*` rewrite.
- **Fork divergence.** This is fork-only now. Upstream made no commits to `pagination_layout.rs` in the last sync window, so the conflict risk is low today. Upstream's effort is moving to the Raikiri backend, so this invests in the Blitz path upstream may leave.
- **Explained: the flex/grid overlap (bug 3) was walker-side, not paint.**
  - In a flex column or grid every item opens a new `RowState` row.
  - The zero-height break-after branch advanced `page_index`, but hit `continue` before the `max_end_*` update.
  - The next item's new-row branch then restored the earlier page, and the pending re-anchor placed it at y=0 there.
  - Render painted it faithfully from that wrong fragment.
  - Fixed locally in fix4. Step 2 would make the shape impossible to write.

## Out of scope (found on the way, tracked here)

- **Floats.** Content after a left float goes below it instead of beside it, with no break involved (ZZZ at 92.1pt vs Prince and Chrome beside the float). This is a layout or paint issue, not pagination. It shows up as 6 matrix misses.
- **Flex rows with multi-line paragraph cells** lose or desync lines when the row crosses a page, before and after every fix here.

## Timing (decided 2026-11-11)

Step 0 lands now, on fork PR #6. Steps 1-4 wait until #5 and #6 are merged. Upstream #959 is closed: no upstream PRs for now.

## Interim state (decided 2026-11-11)

Fork PR #6 carries fix3 and fix4:

- **fix3** fixes bug 2. It matches Prince on 12 more matrix cases than fix2only.
- **fix4** fixes bug 3. 80 of the 84 flex/grid overlap rows now match the reference (Chrome) and are promoted to Pass. The other 4 are `margin-top` cases and carry the separate "mt30 margin-top dropped" label, like their body/section/nest2 siblings.

The earlier known gap, where fix3 turned a flex/grid + `display:none` case into an overlap, is closed.

Gates at fix4:

- `cargo test -p fulgur -p fulgur-blitz -p fulgur-core`: 3009 / 0 / 10.
- CLI tests: 59 / 0.
- clippy and fmt: clean.
- Fixtures: 0 of 86 moved against fix3.
- vol-kap: 67 pages, 0 lines past a page bottom, both headings present. The output is byte-identical to fix3.

Remaining known bugs in the matrix:

- body-level `margin-bottom` (bug 4), 6 rows;
- `margin-top` dropped, 10 rows;
- float layout, 6 rows.

## Step 2 as built (2026-11-12)

Branch `paperworx/walker-place-by-prev`, on top of the phase A matrix rows.

**Placement.** `page_taffy_origin`, `origin_pending_target_y` and `origin_pending_same_row` are gone. The nested walker keeps one `PrevBox { bottom, end, split }` and places a box at `prev.end + (top - prev.bottom)`. A `display: none` child is never the previous box. Rows come from the `flow_rows` pre-pass, and `RowState` snapshots and restores the `PrevBox`. `running_store` is untouched and stays the source of truth for running elements.

**First child.** The held-back first-child fix landed as a spec rule rather than a flag. css-break-3 §3.1 has no break point between a container's top and its first child, so the walker that has a box above the container decides: `leading_unbreakable_extent` measures how far down the container's first unbreakable content reaches, and if that does not fit, the container moves whole, padding included. Deeper levels never guess. What counts as unbreakable:

- a paragraph's first `orphans` lines (the whole paragraph when it is too short to split);
- the cells of the topmost row, picked by geometry and not by DOM order;
- a replaced element;
- nothing, for an empty box with a height.

**Deliberate differences from before**, each matching the body walker or a reference:

- A sibling after a box that crossed a page keeps its gap to it.
- The row after a co-split flex/grid row keeps the row gap.
- A multi-line paragraph's `break-after` preserves the next box's margin.

**Gates.**

- Suite: 3060 / 0 / 10. CLI: 59 / 0.
- Matrix: `table-body`, `table-section`, `block-body` and `block-section` promoted to Pass. The flex/grid and padding guards stay Pass. Fourteen new shapes measured on Prince 17 and Chrome 157: ten Pass, four KnownBug.
- Fixtures: 2 of 98 CLI renders moved (65 VRT fixtures, 21 examples, 12 repros), both to where Prince puts them. VRT in `ubuntu:24.04` named exactly the one golden.
  - `gcpm/string-set`: "Filler 7" was drawn at 782.2pt, below the content box (781.9pt). It now opens page 2 at 81.8pt; Prince has 81.7pt.
  - `examples/transform`: section 5's row was drawn past the page bottom with its labels missing. It is now on page 2 with its labels, as in Prince.
- Review: two independent adversarial reviews, one reading the diff and one rendering 304 shapes against the references. Against the baseline, no shape lost words, 24 show words the baseline dropped, and none has more text near the page bottom.

**Known bugs the matrix now records.**

- **Unsplit leaf** (`pad-leaf-*`, `pad-body-padding.second`, `lead-empty-*`): a box with a height whose content fits is sliced in place by both references. Here it overflows or moves whole.
- **Negative margin after a split box** (`neg-split-*`): Chrome puts the sibling back on the page the split box started on. Here it stays where the split box ended.
- **Row end** (`flex-body.after`, `grid-body.after`): a split cell's last `margin-bottom` is dropped.

**Left for later steps**, from the review:

- "A box is above this one on this page" is still a hand-maintained `box_placed_on`. It should be derived from a `PrevBox` that carries its page, which is also what the negative-margin case needs.
- The move-whole decision is written once per walker, with different capacity and multicol rules. Step 4 should merge them.
- `at_page_top` and `margin_after_forced_break` are one state held as two flags. Folding them into `PrevBox` belongs with step 3, since it changes the body-level margin.

**Found on the way, not pagination.**

- Blitz ignores the `width` / `height` attributes of an inline `<svg style="display:block">`: it is laid out about 300px tall. CSS sizing works.
- A `column-reverse` flex whose items are block containers is walked in DOM order and drawn past the page bottom, before and after this step.
- A container holding a running element or a `string-set` heading near the bottom of a page leaves the next page empty and puts its content one page later. Prince does not.
- Headings do not keep with the next block (`break-after: avoid`), which is the rest of the difference to Prince in `examples/transform`.

## Fragmentation model (design, 2026-11-12)

Written after the paragraph-splitting attempt (branch
`paperworx/paragraph-split-in-place`, parked, two local commits). That fix
derived its rule from Chrome probes and matched Chrome on 110 of them, and two
independent reviewers still found lost lines in paragraphs with padding or an
inline-block. The same class of finding came back in every review round of
step 2. This section steps back: what the spec and Chromium do, what the walker
lacks to do the same, and in which order to add it.

Sources read on 2026-11-12: css-break-3 Editor's Draft (22 Oct 2025),
css-break-4 (14 Apr 2026), css-tables-3, and Chromium `main` under
`third_party/blink/renderer/core/layout/`.

### The rules

| # | Rule | Spec | Chromium |
|---|---|---|---|
| R1 | Breaks are allowed between siblings (class A), between line boxes (class B), and between a box's content edge and its first or last child only where there is a gap (class C) | css-break-3 §3.3: "Class A: Between sibling boxes", "Class B: Between line boxes inside a block container box" | `CalculateBreakAppealBefore`: no separation from the container means "not a valid break point", appeal `kBreakAppealLastResort` |
| R2 | A break between lines keeps `orphans` lines before it and `widows` after it, both 2 by default. If that leaves no break point the restriction is dropped | §4.4 rule 3, and "rule 3 is dropped to provide more break points" | `FinalizeForFragmentation`: `line = max(lines - widows, min(lines, orphans))`. Orphans win a tie: three lines of which two fit break 2 / 1 |
| R3 | `break-inside: avoid` applies to every box except inline-level boxes, so also to table rows, row groups and cells. It is dropped when the box cannot fit a whole page | §3.2, §4.4 | `CalculateBreakAppealInside` gives a break inside the box the appeal `kBreakAppealViolatingBreakAvoid`. The break before the box is better, so the box moves; on a fresh page it breaks inside |
| R4 | A line box, a replaced element, an inline-block and a scroll container are monolithic: no break inside. Monolithic content that does not fit moves to the next page; at the top of a page it stays and overflows | §4.1 "monolithic: it contains no possible break points" | `LayoutBox::IsMonolithic`; `MovePastBreakpoint` "monolithic content at the beginning of the fragmentainer" |
| R5 | At a break no border and no padding are drawn (`box-decoration-break: slice`). The top border and padding belong to the first fragment, the bottom ones to the last | §5.4 "no border and no padding are inserted at a break" | `FinishFragmentation`: `sides.block_start = false` on continuations, `sides.block_end = false` when the box breaks |
| R6 | When the content fits but the bottom padding or border does not, the break moves earlier (lines are pulled over) rather than cutting through the padding. Cutting it is the last resort | — | `FinishFragmentation`: "If we have found a better early break, we should break there. Otherwise mark the break as unappealing" |
| R7 | A box that breaks fills the rest of the page; what it has used counts towards its `height`, and the rest continues on the next page | §5.3 "its content box extends to fill any remaining fragmentainer extent" | `ConsumedBlockSize` in the break token; `desired = total - previously_consumed` |
| R8 | Margins next to an unforced break are dropped; after a forced break the following margin is kept | §5.2 | `AdjustMarginsForFragmentation`; "Margins after a forced break should be retained" |
| R9 | The cells of a table row, and the items of a flex or grid row, are parallel flows: each breaks on its own at the same page boundary, and the row ends where its tallest cell ends | css-tables-3 §6.1 | `table_row_layout_algorithm.cc`: "each cell establishes a parallel flows that needs to be examined separately" |
| R10 | Every page gets some content | §4.4 "must place at least some content on each fragmentainer" | `MovePastBreakpoint`: "we cannot break before this child, or there'd be no progress" |
| R11 | Among the possible breaks, take the one with the highest appeal, and among those the one that fits the most content | §4.4 orders which rules to drop | `break_appeal.h`: `LastResort < ViolatingBreakAvoid < ViolatingOrphansAndWidows < Perfect`; the tutorial: "highest appeal (primary criterion) that also fits as much content as possible" |

Where Chrome departs from the spec, and what we follow:

- **R2, three lines of which two fit.** The spec text prefers moving the
  paragraph; Chrome breaks 2 / 1 and calls it a perfect break. We follow
  Chrome, measured on `para-n3-f2`.
- **Table rows.** css-tables-3 §6.1 says a row under half the page height
  should stay whole. Chrome does not do that: a three-line row breaks 2 / 1
  like a paragraph (measured, `row-n3-f2`). We follow Chrome.
- **Prince 17** uses orphans 1 / widows 1 by default and drops a
  negative-margin sibling after a split. It is a second opinion, not the
  reference, wherever it differs from Chrome.

### What the walker lacks

Chromium's fragmentation rests on four things. The walker has none of them as
a concept; it has local checks that each cover one rule in one place.

| Chromium | fulgur today | Bugs that follow |
|---|---|---|
| A **break token**: what a fragment of a box contains (line range, consumed height) and so which edges it has | A `Fragment` is a rectangle. `render.rs` works out afterwards which lines a rectangle holds, by adding up line heights (`paragraph_lines_for_page`), and draws a full box per slice | Lines lost when the paragraph has padding, a border or an inline-block (pagination measures Parley's line metrics, the renderer re-stacks lines with its own aligned heights); a border drawn on both sides of a cut; the list marker drawn on every slice; a leaf with a `height` cannot be sliced (R7) |
| **Break appeal** on every candidate break, and the best earlier break remembered | `avoid_fits_whole_page`, `leading_unbreakable_extent`, the orphans check, `below_sibling`: one yes/no check per rule, each in both walkers | The rules do not compose. No place for `break-after: avoid` (a heading keeps with the next block), `break-inside: avoid` on a `<tbody>`, or R6 |
| **The page's own height** for each page | One `page_height_px`; page 0's shorter height is corrected in the body walker only | Nested content on page 1 is drawn up to the body's offset below the content box; named pages of another size would do the same |
| **Parallel flows** | `RowState` restores the cursor around a sequential walk of the cells | The co-split findings of step 2; a table without a repeating header did not co-split at all |

### Shape of the implementation: four layers, one direction

Bert's constraint (2026-11-12): the capabilities must not be machinery slapped
onto the walker. They are broken down by kind, each kind lives in one place,
and the walker's main path (a box fits, place it, move on) stays as it is.

Each rule above is one of four kinds of thing, and each kind has one home:

| Layer | What it is | Knows about | Rules | Exists today as |
|---|---|---|---|---|
| **Facts** | Read-only description of the laid-out tree, built before or beside the walk: the boxes of a container in order, what kind each is (monolithic, lines, block container, row of parallel boxes), its line extents, its break properties | Blitz, Taffy, styles | R1, R4, R9, and the inputs of the others | `flow_items`, `flow_rows`, `leading_unbreakable_extent`, `avoids_break_inside` (parked branch) |
| **Decision** | Pure functions: given the facts of one box and the space left, where is the break and how good is it | Numbers only: heights, counts, an appeal. No document, no geometry table | R2, R3, R6, R10, R11 | `lines_before_break` (parked branch); otherwise spread through both walkers as inline `if`s |
| **Walk** | Places boxes in order and carries the cursor. When a box does not fit it asks the decision layer once and acts on the answer | Pages, cursor, previous box | R7, R8 (placement and carried height) | `PrevBox::place`, the two walkers |
| **Record** | What each fragment holds, written by the walk, read by the renderer | Nothing; it is data | R5 | `Fragment` (a rectangle only) |

The dependencies run one way: the walk reads facts and calls decisions; the
renderer reads the record. A decision never touches the geometry table, a fact
never depends on the page, and the renderer never re-derives what the walk
decided.

What this rules out:

- **A rule written as a branch in the walk.** `avoid_fits_whole_page`, the
  orphans pre-check and `below_sibling` are decisions living in the walk,
  once per walker. They move to the decision layer; the walk keeps one
  question: "this box does not fit: break before it, inside it, or place it
  anyway?"
- **Cost on the main path.** Facts beyond the box list and every decision are
  computed only when a box does not fit. A document that fits its pages runs
  the walk it runs today.
- **A fix in the renderer for a pagination decision**, such as
  `is_continuation_fragment` on the parked branch, which guesses from the
  fragment list what the walk already knew.
- **State added to the walk for one case.** New walk state has to be one of:
  the cursor, the previous box, or the page. Anything else is a fact (compute
  it) or a record (write it).

Each layer is tested at its own level: decisions with plain numbers and no
document, facts against a parsed document, the walk and record through the
break matrix.

### What to add, in this order

Each step is a capability, not a case. Each keeps the output contract of
"What must not change" above and reports its blast radius the same way. Each
names the layer it adds to: step 1 is the record, step 2 the walk, steps 3 and
5 the decision layer (step 5 also moves the existing inline checks into it),
step 4 the walk's page, step 6 the walk itself.

1. **Fragment content (the break token).** `PaginationGeometry` records, per
   fragment, what it holds: for a paragraph the line range and the top of its
   first line; for any box whether the fragment has the box's start edge and
   its end edge. The renderer draws that and derives nothing. Pagination
   measures lines with the heights the renderer stacks them with, so the two
   cannot disagree.
   Fixes: lost lines (padding, border, inline-block), borders at a cut, the
   marker (replaces `is_continuation_fragment`), and makes R5 one rule.
2. **Consumed height (R7).** A box that breaks fills the page and carries its
   remaining height. One slicing routine for a leaf with a `height`, used by
   both walkers, replacing the body walker's oversized-only slice loop.
   Fixes: the five "leaf block" matrix rows, and a paragraph with a `height`
   larger than its lines.
3. **Split paragraphs in place (R2, R6).** The parked branch, rebuilt on 1 and
   2: `lines_before_break` is already Chromium's formula. Adds R6 (pull lines
   over rather than strand the bottom padding).
4. **Page heights.** The nested walker takes the height of the page it is on.
   Replaces the page-0 shift in the body walker.
5. **Break appeal (R3, R11).** One function ranks the break before a box
   against the best break inside it. `avoid` on a row, a row group and a cell,
   and `break-after: avoid`, become inputs to it instead of separate checks.
   This is the step that needs the walker to look ahead: it can, because all
   geometry is fixed before pagination starts, so the break points of a
   subtree can be listed without laying anything out again.
6. **One walker**, as planned in step 4 above, with rows as parallel flows
   (R9) rather than a restored cursor.

Steps 1–3 remove the content-loss class. Steps 4–6 remove the reasons the
review rounds keep finding the same kind of problem.

### Decisions (Bert, 2026-11-12)

1. **Order.** Steps 1–3 first, then 4–6.
2. **PR #10** is merged into `paperworx/repros` (squash, `e39889ac`); the
   steps branch from there.
3. **`break-after: avoid` and `break-before: avoid`** are parsed as part of
   step 5 (`BreakBefore` and `BreakAfter` in `draw_primitives.rs` have only
   `Auto` and `Page` today).
4. **Upstream.** Fork-only. Step 1 changes `PaginationGeometry`, which makes a
   later upstream sync of `pagination_layout.rs` and `render.rs` harder; that
   cost is accepted.

### Measured, to carry into the steps

- The parked branch matches Chrome on 110 plain-paragraph probes, fixes
  paperworx repro 16 (a table row splits; `avoid` on a `<tr>` holds), and adds
  18 matrix rows. Its `lines_before_break`, `paragraph_leading_height`,
  `avoids_break_inside` and the plain-table co-split carry over.
- Its known failures, all from the missing break token: a padded or bordered
  paragraph split mid-page loses a line or draws one past the page bottom; a
  paragraph with an inline-block loses 3–6 words; a negative body offset
  misplaces page 1; `avoid` on a `<tbody>` is not honoured.
- Found on the way and older than this work: at body level, the block after a
  multi-line paragraph with padding or a border overlaps it by the padding
  height; a container's bottom padding and border are dropped after its last
  child split; a table cell's `height` shorter than its content does not grow
  the row (Blitz layout, not pagination).
