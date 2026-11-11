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
