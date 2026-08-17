# refdiff — measured feature gaps against WeasyPrint

Renders each fixture through fulgur and WeasyPrint, compares the text each
engine actually draws and where it lands, and regenerates
`docs/css-reference-diff.md`.

```bash
cargo build -p fulgur-cli
python3 scripts/refdiff/refdiff.py            # regenerate the doc
python3 scripts/refdiff/refdiff.py --print    # stdout only
python3 scripts/refdiff/refdiff.py -k table   # only matching fixtures
```

Needs `pdfinfo`, `pdftotext` (poppler) and `uvx` on PATH. Not wired into CI:
WeasyPrint is not a build dependency, and this is a research tool for finding
work, not a gate.

## Why it is generated rather than written

The claim "fulgur never implemented `<thead>` repetition" was stated in two
source comments, `paperworx-repros/README.md` and `CLAUDE.md`. It was wrong in
all four — the feature had shipped in the v1 `Pageable` architecture and was
dropped in the Phase 4 migration. Hand-maintained capability lists drift
silently and then mislead confidently. A measured one cannot drift the same way.

That is also this repo's own rule: *never trust a README over a measurement*.

## What the statuses mean

Ranked by how much a row actually tells you:

| status | meaning | act on it? |
|---|---|---|
| `MISSING` | one engine draws content the other does not | yes |
| `PAGES` | different page count | yes |
| `PLACEMENT` | unique content in a different reading order | yes |
| `PACKING` | same reading order, page boundaries elsewhere | usually font metrics |
| `SHIFT` | same content and pages, positions differ | usually font metrics |
| `AGREE` | same content, same pages, positions within tolerance | no |

The `PACKING` / `PLACEMENT` split is what keeps the output honest. The two
engines resolve different default fonts, so line heights differ and each fits a
different amount per page. That alone shifts a tail of content across a boundary
in almost every multi-page fixture — real, but nothing to do with the feature
under test. So the order comparison runs over **words that appear exactly once
in the document**: a repeated `<thead>`, a margin box or a `position: fixed`
mark occurs once per page and moves whenever a boundary moves, which would make
every correctly-repeating fixture look broken.

## Fixtures

One variable each (`fixtures/*.html`), following the rule in `CLAUDE.md`:
sixteen one-slot files beat one sixteen-slot file. Each is self-contained, A4,
and uses short distinct words so neither engine hyphenates and breaks the
word-level comparison.

Adding one: drop an HTML file in `fixtures/`, give it a `<title>` describing the
single thing it isolates (the title becomes the table's second column), and
regenerate.

## A divergence is a lead, not a verdict

WeasyPrint is not the spec. Confirm against a second reference before filing —
Chrome 151 headless-shell agrees with WeasyPrint to ~0.1mm on every case tested
so far, and the one place the two disagree with each other is documented in
`CLAUDE.md`. Two references agreeing is the bar this repo uses.

Its first run caught two things: it independently rediscovered the `<tfoot>`
source-order defect (found by hand while implementing footer repetition), and
surfaced a `break-inside: avoid` overflow that nothing else had.
