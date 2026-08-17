# DEFECT 3 — silent blank text when no registered font matches (WASM)

**The most dangerous of the four: it fails with no error at all.**

WASM has no system fonts. When the CSS `font-family` names a family that has not been
registered via `Engine.add_font()`, every glyph renders as *nothing* — and the render
reports success.

## Observed (fulgur-wasm 0.40.0 in workerd 1.20250718.0)

Document: 16.5 KB, 40 paragraphs, `font-family: Georgia, "Times New Roman", serif`.
Only `NotoSans-Regular.ttf` was registered.

```
renderMs: [6, 7, 8]          <- looks healthy
pdfBytes: 1697               <- 1 page, ZERO text
htmlBytes: 16499             <- input arrived intact
```

Remapping the CSS to `font-family: "Noto Sans"` — the registered family — gave
6 pages / 40 paragraphs / 6 footers, matching the native CLI, at 24 ms.

## Why it matters

For documents of record (financial filings, share certificates, signed contracts) a
blank page that reports success is worse than a hard error. A caller cannot distinguish
"rendered correctly" from "rendered nothing".

## Suggested fix

Either fall back to any registered font when the requested family has no match, or
return an error / warning naming the unmatched family. Silence is the bug.
