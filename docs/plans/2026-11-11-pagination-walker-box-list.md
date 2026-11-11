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
