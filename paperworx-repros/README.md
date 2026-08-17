# paperworx repros

Four defects found while evaluating fulgur as a PDF engine for
[paperworx](https://github.com/StudioMaak/paperworx), 2026-08-16. Each was measured
against WeasyPrint 69 and Chrome 151 (headless-shell) on identical input; both of those
agree with each other, so they are the reference.

Intent: fix here, then offer upstream as separate PRs.

| # | defect | file | severity |
|---|---|---|---|
| 1 | Running elements not repeated per page | `01-running-element-per-page.html` | blocks NBB |
| 2 | `<thead>` not repeated on continuation pages | `02-thead-repeat.html` | blocks NBB |
| 3 | Silent blank text when no registered font matches | `03-font-miss-silent-blank.md` | **dangerous** |
| 4 | Margin-box slots not anchored to their named position | `04-margin-box-anchor.html` | blocks NBB |

## How they were measured

```bash
# reference engines
uvx --from weasyprint weasyprint in.html out-weasy.pdf
# fulgur
cargo run -p fulgur-cli -- render -o out-ful.pdf in.html
# compare text positions in mm
pdftotext -bbox -f 1 -l 1 out.pdf -    # xMin/yMin are pt; * 25.4/72 = mm
```
