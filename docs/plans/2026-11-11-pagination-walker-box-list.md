# Pagination walker: place boxes, stop re-anchoring

Status: proposal, fork-only (no upstream PRs for now). 2026-11-11.

## Why

Four bugs in a row came from one mechanism, and each fix was a local rule guessed from the shape that broke:

| # | Shape | Symptom | Fix so far |
|---|---|---|---|
| 1 | forced break, then a `display:none` / running sibling, then content (vol-kap) | content placed at its unbroken offset: pushed a page on, or painted below the page | defer the re-anchor past zero-height children (`16451d6d`) |
| 2 | forced break, then a real zero-height box with `margin-bottom` | the margin is dropped (39.6pt vs Prince 61.2pt) | defer only past `display:none` (fix3, on fork PR #6) |
| 3 | forced break inside flex/grid, then a `display:none` sibling | content drawn on top of earlier content on the same page | none; fix3 makes this worse than base |
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
| fix2only (PR #6 today) | 102 | 24 |
| fix3 (display:none only) | 114 | 12 |

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

0. **A checked-in break matrix.** Move the matrix into `crates/fulgur/tests/break_matrix.rs`. The expected page and y come from the reference engines and are written down as literals, never recomputed. Cases that fail today are `#[ignore]`d with the bug number, so every later step can un-ignore what it fixes.
   - Decide the two cases where the references disagree, here and once:
     - Flex/grid forced breaks: follow Chrome. Prince ignores the break.
     - A zero-height box's `margin-top` after a forced break: follow Chrome and keep it. Prince drops it but keeps `margin-bottom`, which is inconsistent for a margin that collapses through.
   - Also fix the CodeRabbit point here: tests that shell out to `pdftotext` fail when `CI` is set instead of silently passing.
1. **The flow-list pre-pass, with no behaviour change.** Add `flow_items(doc, parent) -> Vec<FlowItem>`. Have the body walker, the nested walker, `would_split_block_subtree` and `has_page_name_change_below` iterate it instead of the raw children.
   - Gate: 0 fixtures moved, suite unchanged, matrix unchanged.
2. **The nested walker places by previous box.** Replace `page_taffy_origin`, `origin_pending_target_y` and `origin_pending_same_row` with `prev_end` + `gap_before`, matching the body walker. Rebuild `RowState` on the pre-computed rows.
   - Expected wins: bugs 1-3 and the flex/grid overlap cases.
   - Gate: matrix un-ignores those cases, suite green, and every moved fixture explained.
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
- **Unexplained: the flex/grid overlap.** The zero-height break-after branch does advance `page_index` (a blank trailing page appears), yet the next item is painted on the previous page. Either the flex path places it before the break takes effect, or paint ignores the item's own fragment. Find out at the start of step 2: if it is the paint side, step 2 alone won't fix it.

## Interim state (decided 2026-11-11)

Fork PR #6 carries fix3, which fixes bug 2 and matches Prince on 12 more matrix cases than fix2only. **Known gap:** it turns a flex/grid + forced break + `display:none` case from "break ignored" into "overlap" (ZZZ at 39.6pt on top of AAA, against 84.6pt on base). The vol-kap filing does not hit that shape, and step 2 is what fixes it.

Gates at fix3:

- `cargo test -p fulgur -p fulgur-blitz -p fulgur-core`: 3006 / 0 / 10.
- CLI tests: 59 / 0.
- clippy and fmt: clean.
- Fixtures: 0 of 86 moved against fix2only.
- vol-kap: 67 pages, 0 lines past a page bottom, both headings present.
