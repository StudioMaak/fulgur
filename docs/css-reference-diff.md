# Measured feature gaps against WeasyPrint

**Generated — do not edit by hand.** Regenerate with:

```bash
cargo build -p fulgur-cli && python3 scripts/refdiff/refdiff.py
```

Reference: `WeasyPrint version 69.0`. Fixtures: `scripts/refdiff/fixtures/`,
one variable each.

This is a **gap finder, not a conformance oracle**. WeasyPrint is not the
spec, and the two engines resolve different fonts, so absolute positions
always differ. Statuses are ranked by how much they mean:

| status | meaning | worth acting on |
|---|---|---|
| `MISSING` | one engine draws content the other does not | yes |
| `PAGES` | different page count | yes |
| `PLACEMENT` | reading order itself differs | yes |
| `PACKING` | same reading order, page boundaries elsewhere | usually font metrics |
| `SHIFT` | same content and pages, max Δy > 12pt | usually font metrics |
| `AGREE` | same content, same pages, positions within tolerance | no |

Summary: **1 MISSING**, **5 PACKING**, **2 SHIFT**, **6 AGREE**.

| fixture | what it isolates | status | fulgur | weasy | detail |
|---|---|---|---|---|---|
| `table-tfoot-source-order` | tfoot written before tbody must still render last | `MISSING` | 3 | 3 | fulgur omits `Foot` (1 of 3), `Total` (1 of 3) |
| `list-markers` | ordered list markers across a page break | `PACKING` | 2 | 2 | 6 runs cross a page boundary differently (reading order identical) |
| `orphans-widows` | orphans and widows defaults at a page boundary | `PACKING` | 2 | 2 | 66 runs cross a page boundary differently (reading order identical) |
| `table-caption` | caption placement across a page break | `PACKING` | 3 | 3 | 20 runs cross a page boundary differently (reading order identical) |
| `table-tfoot-repeat` | tfoot repeats on continuation pages | `PACKING` | 3 | 3 | 20 runs cross a page boundary differently (reading order identical) |
| `table-thead-repeat` | thead repeats on continuation pages | `PACKING` | 3 | 3 | 20 runs cross a page boundary differently (reading order identical) |
| `multicol-basic` | two-column flow | `SHIFT` | 1 | 1 | max Δy 109.2pt (`Mc66`) |
| `table-cell-break-before` | break-before:page on a table cell | `SHIFT` | 1 | 1 | max Δy 42.8pt (`R0039`) |
| `break-before-page` | break-before:page forces a new page | `AGREE` | 3 | 3 | max Δy 10.3pt |
| `break-inside-avoid` | break-inside:avoid keeps a block whole | `AGREE` | 3 | 3 | max Δy 1.3pt |
| `counter-page` | counter(page) in a margin box | `AGREE` | 2 | 2 | max Δy 0.8pt |
| `page-margin-boxes` | @page margin boxes on every page | `AGREE` | 2 | 2 | max Δy 0.8pt |
| `position-fixed` | position:fixed repeats on every page | `AGREE` | 2 | 2 | max Δy 5.3pt |
| `running-element` | position:running() hoisted into a margin box | `AGREE` | 2 | 2 | max Δy 1.8pt |

## Reading a divergence

A `MISSING` or `PLACEMENT` row is a lead, not a verdict. Confirm it against a
second reference before filing: Chrome 151 headless-shell agrees with
WeasyPrint to ~0.1mm on every case tested so far, and the one place they
disagree with each other (the margin box's box model) is documented in
`CLAUDE.md`. Two references agreeing is the bar this repo uses.

See `docs/test-harnesses.md` for what each test harness asserts, and
`docs/css-support.md` for hand-written per-property notes.
