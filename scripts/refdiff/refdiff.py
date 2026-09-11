#!/usr/bin/env python3
"""Differential capability probe: fulgur against WeasyPrint.

Renders each fixture through both engines, extracts word boxes with
``pdftotext -bbox``, and reports where the two disagree. The output is a
measured gap list, regenerated on demand — not a hand-maintained one, which is
how ``docs/css-support.md`` and the "fulgur never implemented <thead> repetition"
claim both went stale.

This is a *gap finder, not a conformance oracle*. WeasyPrint is not the spec,
and the two engines resolve different fonts, so absolute positions always
differ. The signals are ranked accordingly:

  MISSING    content one engine draws and the other does not      strong
  PAGES      the document paginates to a different page count     strong
  PLACEMENT  reading order itself differs                         strong
  PACKING    same reading order, page boundaries fall elsewhere   weak (fonts)
  SHIFT      same content and pages, positions differ             weak (fonts)
  AGREE      same content, same pages, positions within tolerance

Only the first three are worth opening an issue over without further work.
PACKING and SHIFT are reported for completeness: both fall out of the engines
resolving different default fonts, so line heights — and therefore how much
fits on a page — differ before any layout question is reached.

Usage:
    python3 scripts/refdiff/refdiff.py                  # write docs/css-reference-diff.md
    python3 scripts/refdiff/refdiff.py --print          # stdout only
    python3 scripts/refdiff/refdiff.py -k table         # only fixtures matching
"""

from __future__ import annotations

import argparse
import re
import shutil
import subprocess
import sys
import tempfile
import xml.etree.ElementTree as ET
from collections import Counter
from dataclasses import dataclass, field
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent.parent
FIXTURES = HERE / "fixtures"
OUT_DOC = ROOT / "docs" / "css-reference-diff.md"

# Positions below this delta (pt) are treated as font-metric noise rather than
# a placement difference. Two engines with different fonts routinely differ by
# a few points on the same logically-placed run.
SHIFT_TOLERANCE_PT = 12.0

NS = "{http://www.w3.org/1999/xhtml}"


@dataclass
class Render:
    pages: int
    # page index (1-based) -> Counter of word strings
    words: dict[int, Counter] = field(default_factory=dict)
    # (page, word) -> list of (xMin, yMin)
    boxes: dict[tuple[int, str], list[tuple[float, float]]] = field(default_factory=dict)

    # flattened reading order across all pages
    sequence: list[str] = field(default_factory=list)

    def all_words(self) -> Counter:
        total: Counter = Counter()
        for c in self.words.values():
            total.update(c)
        return total


def run(cmd: list[str], **kw) -> subprocess.CompletedProcess:
    return subprocess.run(cmd, capture_output=True, text=True, **kw)


def page_count(pdf: Path) -> int:
    out = run(["pdfinfo", str(pdf)]).stdout
    m = re.search(r"Pages:\s+(\d+)", out)
    return int(m.group(1)) if m else 0


def extract(pdf: Path) -> Render:
    n = page_count(pdf)
    r = Render(pages=n)
    for p in range(1, n + 1):
        xml = run(["pdftotext", "-bbox", "-f", str(p), "-l", str(p), str(pdf), "-"]).stdout
        counter: Counter = Counter()
        try:
            tree = ET.fromstring(xml)
        except ET.ParseError:
            r.words[p] = counter
            continue
        for word in tree.iter(NS + "word"):
            text = (word.text or "").strip()
            if not text:
                continue
            counter[text] += 1
            r.sequence.append(text)
            key = (p, text)
            r.boxes.setdefault(key, []).append(
                (float(word.get("xMin")), float(word.get("yMin")))
            )
        r.words[p] = counter
    return r


def render_fulgur(html: Path, out: Path, binary: Path) -> str | None:
    proc = run([str(binary), "render", "-o", str(out), str(html)])
    if proc.returncode != 0 or not out.exists():
        return (proc.stderr or "render failed").strip().splitlines()[-1:][0] if proc.stderr else "render failed"
    return None


def render_weasy(html: Path, out: Path) -> str | None:
    proc = run(["uvx", "--from", "weasyprint", "weasyprint", str(html), str(out)])
    if proc.returncode != 0 or not out.exists():
        return (proc.stderr or "render failed").strip().splitlines()[-1:][0] if proc.stderr else "render failed"
    return None


@dataclass
class Result:
    name: str
    title: str
    status: str
    detail: str
    ful_pages: int
    weasy_pages: int


def page_of(render: Render, word: str) -> int | None:
    """First page carrying `word`, or None."""
    for p in sorted(render.words):
        if render.words[p].get(word):
            return p
    return None


def compare(name: str, title: str, ful: Render, weasy: Render) -> Result:
    ful_all, weasy_all = ful.all_words(), weasy.all_words()

    only_weasy = weasy_all - ful_all
    only_ful = ful_all - weasy_all
    if only_weasy or only_ful:
        bits = []
        if only_weasy:
            bits.append("fulgur omits " + fmt_words(only_weasy, ful_all, weasy_all))
        if only_ful:
            bits.append("fulgur adds " + fmt_words(only_ful, ful_all, weasy_all))
        return Result(name, title, "MISSING", "; ".join(bits), ful.pages, weasy.pages)

    if ful.pages != weasy.pages:
        return Result(
            name, title, "PAGES",
            f"fulgur {ful.pages} pages, WeasyPrint {weasy.pages}",
            ful.pages, weasy.pages,
        )

    # Same content, same page count: is it in the same order, and on the same
    # pages?
    #
    # Distinguishing these two matters. If the flattened reading order is
    # identical and only the page boundaries fall elsewhere, the engines simply
    # fit different amounts per page — which follows from their different
    # default fonts and says nothing about the feature under test. If the
    # reading order itself differs, something is genuinely in the wrong place.
    misplaced = []
    for word in weasy_all:
        wp, fp = page_of(weasy, word), page_of(ful, word)
        if wp != fp:
            misplaced.append((word, fp, wp))
    if misplaced:
        misplaced.sort(key=lambda t: t[0])
        shown = ", ".join(f"`{w}` p{f}→p{x}" for w, f, x in misplaced[:3])
        more = f" (+{len(misplaced) - 3} more)" if len(misplaced) > 3 else ""
        if unique_order(ful) == unique_order(weasy):
            return Result(
                name, title, "PACKING",
                f"{len(misplaced)} runs cross a page boundary differently "
                f"(reading order identical)",
                ful.pages, weasy.pages,
            )
        return Result(name, title, "PLACEMENT", f"{shown}{more}", ful.pages, weasy.pages)

    # Same content, same pages: report the worst positional delta as FYI.
    worst, worst_word = 0.0, ""
    for (p, word), fboxes in ful.boxes.items():
        wboxes = weasy.boxes.get((p, word))
        if not wboxes:
            continue
        d = abs(fboxes[0][1] - wboxes[0][1])
        if d > worst:
            worst, worst_word = d, word
    if worst > SHIFT_TOLERANCE_PT:
        return Result(
            name, title, "SHIFT",
            f"max Δy {worst:.1f}pt (`{worst_word}`)",
            ful.pages, weasy.pages,
        )
    return Result(name, title, "AGREE", f"max Δy {worst:.1f}pt", ful.pages, weasy.pages)


def unique_order(r: Render) -> list[str]:
    """Reading order of words that appear exactly once in the document.

    Filtering to unique words is what makes the order comparison meaningful in
    the presence of per-page repetition. A repeated `<thead>`, a margin box or a
    `position: fixed` mark occurs once per page, so its position in the
    flattened sequence moves whenever a page boundary moves — which would make
    every correctly-repeating fixture look structurally wrong. Unique body
    content has no such ambiguity: if it changes order, something really is in
    the wrong place.
    """
    counts = r.all_words()
    return [w for w in r.sequence if counts[w] == 1]


def fmt_words(diff: Counter, ful_all: Counter, weasy_all: Counter, limit: int = 4) -> str:
    """Render a multiset difference as "drawn n of m times", which reads far
    better than a bare delta when a band is drawn once instead of per page."""
    items = sorted(diff.items())
    shown = ", ".join(
        f"`{w}` ({ful_all.get(w, 0)} of {weasy_all.get(w, 0)})" for w, _ in items[:limit]
    )
    return shown + (f" (+{len(items) - limit} more)" if len(items) > limit else "")


def title_of(html: Path) -> str:
    m = re.search(r"<title>(.*?)</title>", html.read_text(), re.S)
    return m.group(1).strip() if m else html.stem


ORDER = {"MISSING": 0, "PAGES": 1, "PLACEMENT": 2, "PACKING": 3, "SHIFT": 4, "AGREE": 5, "ERROR": 6}


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--print", action="store_true", dest="to_stdout", help="print instead of writing the doc")
    ap.add_argument("-k", dest="filter", default="", help="only fixtures whose name contains this")
    ap.add_argument("--fulgur-bin", default=str(ROOT / "target" / "debug" / "fulgur"))
    args = ap.parse_args()

    for tool in ("pdfinfo", "pdftotext", "uvx"):
        if not shutil.which(tool):
            print(f"error: `{tool}` not found on PATH", file=sys.stderr)
            return 2
    binary = Path(args.fulgur_bin)
    if not binary.exists():
        print(f"error: fulgur binary not found at {binary}\n"
              f"       build it with: cargo build -p fulgur-cli", file=sys.stderr)
        return 2

    fixtures = sorted(FIXTURES.glob("*.html"))
    if args.filter:
        fixtures = [f for f in fixtures if args.filter in f.name]
    if not fixtures:
        print("error: no fixtures matched", file=sys.stderr)
        return 2

    results: list[Result] = []
    with tempfile.TemporaryDirectory() as tmp:
        tmpdir = Path(tmp)
        for html in fixtures:
            title = title_of(html)
            print(f"  {html.stem} ...", file=sys.stderr, flush=True)
            fp, wp = tmpdir / f"{html.stem}-ful.pdf", tmpdir / f"{html.stem}-weasy.pdf"
            err = render_fulgur(html, fp, binary) or render_weasy(html, wp)
            if err:
                results.append(Result(html.stem, title, "ERROR", err, 0, 0))
                continue
            results.append(compare(html.stem, title, extract(fp), extract(wp)))

    results.sort(key=lambda r: (ORDER.get(r.status, 9), r.name))
    doc = render_doc(results)
    if args.to_stdout:
        print(doc)
    else:
        OUT_DOC.write_text(doc)
        print(f"wrote {OUT_DOC.relative_to(ROOT)}", file=sys.stderr)
    return 0


def render_doc(results: list[Result]) -> str:
    counts = Counter(r.status for r in results)
    weasy_ver = run(["uvx", "--from", "weasyprint", "weasyprint", "--version"]).stdout.strip()
    lines = [
        "# Measured feature gaps against WeasyPrint",
        "",
        "**Generated — do not edit by hand.** Regenerate with:",
        "",
        "```bash",
        "cargo build -p fulgur-cli && python3 scripts/refdiff/refdiff.py",
        "```",
        "",
        f"Reference: `{weasy_ver or 'WeasyPrint'}`. Fixtures: `scripts/refdiff/fixtures/`,",
        "one variable each.",
        "",
        "This is a **gap finder, not a conformance oracle**. WeasyPrint is not the",
        "spec, and the two engines resolve different fonts, so absolute positions",
        "always differ. Statuses are ranked by how much they mean:",
        "",
        "| status | meaning | worth acting on |",
        "|---|---|---|",
        "| `MISSING` | one engine draws content the other does not | yes |",
        "| `PAGES` | different page count | yes |",
        "| `PLACEMENT` | reading order itself differs | yes |",
        "| `PACKING` | same reading order, page boundaries elsewhere | usually font metrics |",
        f"| `SHIFT` | same content and pages, max Δy > {SHIFT_TOLERANCE_PT:.0f}pt | usually font metrics |",
        "| `AGREE` | same content, same pages, positions within tolerance | no |",
        "",
        "Summary: " + ", ".join(f"**{counts[s]} {s}**" for s in ORDER if counts.get(s)) + ".",
        "",
        "| fixture | what it isolates | status | fulgur | weasy | detail |",
        "|---|---|---|---|---|---|",
    ]
    for r in results:
        fp = str(r.ful_pages) if r.ful_pages else "-"
        wp = str(r.weasy_pages) if r.weasy_pages else "-"
        lines.append(f"| `{r.name}` | {r.title} | `{r.status}` | {fp} | {wp} | {r.detail} |")
    lines += [
        "",
        "## Reading a divergence",
        "",
        "A `MISSING` or `PLACEMENT` row is a lead, not a verdict. Confirm it against a",
        "second reference before filing: Chrome 151 headless-shell agrees with",
        "WeasyPrint to ~0.1mm on every case tested so far, and the one place they",
        "disagree with each other (the margin box's box model) is documented in",
        "`CLAUDE.md`. Two references agreeing is the bar this repo uses.",
        "",
        "See `docs/test-harnesses.md` for what each test harness asserts, and",
        "`docs/css-support.md` for hand-written per-property notes.",
    ]
    return "\n".join(lines) + "\n"


if __name__ == "__main__":
    raise SystemExit(main())
