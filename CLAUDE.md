# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Fulgur is an HTML/CSS to PDF conversion library and CLI tool written in Rust. It uses Blitz for HTML parsing/layout, Krilla for PDF generation, and Taffy/Parley for layout/text shaping.

## Common Commands

```bash
# Build
cargo build
cargo build --release

# Test
cargo test --lib                   # note: in the workspace root this runs only fulgur-vrt
cargo test -p fulgur-blitz --lib   # Blitz backend unit tests (the bulk of the pipeline)
cargo test -p fulgur-core --lib    # backend-independent unit tests (config, assets, units, inspect)
cargo test -p fulgur               # facade integration tests (crates/fulgur/tests)
cargo test -p fulgur --test gcpm_integration

# Lint
cargo clippy
cargo fmt --check
npx markdownlint-cli2 '**/*.md'

# Run CLI
cargo run --bin fulgur -- render input.html -o output.pdf
cargo run --bin fulgur -- render input.html --size A4 --landscape -o output.pdf
```

## Architecture

The processing pipeline flows:

```text
HTML string → Blitz (parse/style/layout) → Drawables / PaginationGeometryTable → Page splitting → Krilla PDF
```

### Workspace Structure

- `crates/fulgur/` — Public facade crate. Re-exports `fulgur-blitz` (and through it `fulgur-core`) so existing paths such as `fulgur::Engine` and `fulgur::asset::AssetBundle` stay stable. Holds the integration tests
- `crates/fulgur-core/` — Backend-independent parts: `config`, `asset`, `units`, `error`, `image`, `inspect`. Must not depend on Blitz, Stylo, Taffy, Parley, or Raikiri
- `crates/fulgur-blitz/` — The current Blitz-based pipeline (parse, layout, pagination, drawables, PDF rendering, GCPM, templates)
- `crates/fulgur-cli/` — CLI binary using clap
- `crates/fulgur-raikiri/` — The Raikiri backend. Published in the `core` version group, and depends on the crates.io releases of `raikiri-html`, `raikiri-net` and `raikiri-traits` (no git, path or `[patch]` dependency on Raikiri). Nothing else that is published depends on it yet; the facade, CLI and bindings still use Blitz only
- `crates/fulgur-dev/` — Unpublished development CLI that can select either backend (`--engine raikiri`)

The contract between layout backends is "HTML + assets + config → PDF bytes"; backends do not share an intermediate representation. `fulgur-core` stays backend-independent. A Raikiri update is an ordinary dependency bump of the `raikiri-*` version requirements in `crates/fulgur-raikiri/Cargo.toml`.

### Key Modules (fulgur-blitz unless noted)

- **engine.rs** — `Engine` builder: configures and executes `render()`
- **blitz_adapter.rs** — Thin adapter isolating Blitz API changes from the rest of the codebase
- **convert.rs** — Transforms Blitz DOM nodes into `Drawables` and geometry records
- **draw_primitives.rs** — Primitive geometry, canvas, style, and drawing-helper types (`Size`, `Rect`, `Canvas`, `BlockStyle`, background/gradient types, etc.)
- **drawables.rs** — `Drawables` struct: flat per-node draw payloads (`BlockDraw`, `ParagraphDraw`, `ImageDraw`, etc.) used by the v2 render path
- **paginate.rs** — Page splitting algorithm that walks the `PaginationGeometryTable`
- **render.rs** — Draws paginated fragments onto Krilla surfaces via `render_v2`
- **config.rs** (fulgur-core) — Page size, margins, orientation, metadata
- **asset.rs** (fulgur-core) — `AssetBundle` manages CSS, fonts, and images (offline-first, all assets explicitly registered)
- **paragraph.rs** — Text line layout and drawing
- **gcpm/** — CSS Generated Content for Paged Media: parser, margin boxes, running elements, counters

### Design Principles

- **Offline-first**: No network access; all assets must be explicitly bundled
- **Deterministic**: Same input always produces same output — see the font caveat below
- **Hybrid layout**: Taffy pre-computes sizes; geometry is recorded in `PaginationGeometryTable` once and reused during pagination — no re-layout after splitting
- **Adapter isolation**: Blitz API surface is contained in `blitz_adapter.rs`

**Font determinism caveat**: `blitz-dom` 0.2.4 hardcodes
`fontdb::Database::load_system_fonts()` for inline `<svg>` parsing (see
`blitz-dom-0.2.4/src/util.rs`), and fulgur currently inherits Parley's
system font fallback for HTML text whenever no bundled fonts are supplied.
This means the same HTML can produce different PDFs on two hosts if their
installed `.ttf`/`.otf` set differs — the usual bite is `<text>` inside
SVG picking a fallback that happens to ship on one machine but not the
other. The regeneration scripts under `mise.toml` and
`.github/workflows/update-examples.yml` pin this via
`FONTCONFIG_FILE=examples/.fontconfig/fonts.conf`, which redirects
fontconfig to the bundled Noto Sans set in `examples/.fonts/`. When
editing fonts, CLI defaults, or the SVG pipeline, remember that library
callers don't get this guarantee by default — see the tracking issue
`fulgur-a8s` and the README's *Determinism and fonts* section.

### Gotchas

- **Coordinate system and unit conversion**: fulgur uses three distinct unit spaces
  (Blitz/Taffy in CSS px, Pageable/Krilla in PDF pt, `PageSize::custom` in mm).
  Forgetting a conversion is the most common source of scale bugs (4/3× or 3/4× off).
  See `.claude/rules/coordinate-system.md` for the full rules, conversion helpers,
  and known pitfalls (Krilla Y-down, Stylo px basis, CSS transform composition,
  PDF text-space operators).
- **Blitz is thread-safe** (contrary to earlier belief). Multiple threads can
  call `blitz_adapter::parse` / `resolve` / `apply_passes` concurrently on
  independent documents. The previous "Blitz not thread-safe" note was based
  on a misdiagnosis — the real race was in fulgur's own `suppress_stdout`
  helper, which has been removed. See
  `docs/plans/2026-04-11-blitz-thread-safety-investigation.md` for the full
  root-cause analysis.
- **Blitz prints html5ever parse errors via `println!` to stdout** during
  `TreeSink::finish`. This is noise from dependencies, not fulgur.
  Policy by crate:
  - **`crates/fulgur` (core library)** must not touch fd 1 under any
    circumstance. `blitz_adapter::suppress_stdout` was removed for this
    reason (see
    `docs/plans/2026-04-11-blitz-thread-safety-investigation.md`).
  - **`crates/fulgur-cli`** is single-threaded during render and may
    manipulate fd 1 via `StdoutIsolator` — this is required for
    correctness (`-o -` writes PDF bytes to stdout; any noise corrupts
    the stream).
  - **`crates/pyfulgur`, `crates/fulgur-ruby`** are multi-threaded
    bindings. They must not manipulate fd 1 either: a global suppress
    mutex still races with `suppress=false` callers on the same process,
    and PDF bytes are returned via the function return value (not
    stdout), so noise is cosmetic, not a correctness issue. The
    canonical workaround for binding users is redirection at their own
    call site (e.g. `os.dup2` / `contextlib.redirect_stdout`) or running
    renders in a subprocess (`multiprocessing`). A future wrapper-style
    package that shells out to the CLI is on the roadmap for users who
    want clean stdout without doing this themselves.
  - Short version: **touch fd 1 only from a crate that can guarantee
    single-threaded semantics**. That's CLI today; bindings are
    multi-threaded by design and must leave fd 1 alone.
- **Worktree sparse-checkout**: `git worktree add` inherits a `/.beads/`-only
  sparse-checkout pattern, which makes `git add` refuse modifications to
  source files (`paths ... outside of your sparse-checkout definition`).
  Two ways to deal with this:
  - Right after `git worktree add <path> -b <branch>`, run
    `git -C <path> sparse-checkout disable`. This is the recommended fix —
    it sets `core.sparseCheckout=false` for that worktree only.
  - If you've already started work and only need a one-off commit, use
    `git add --sparse <files>` to force the index update.
  The `EnterWorktree` tool's PostToolUse hook in `.claude/settings.json`
  handles this automatically when it's used to enter a worktree, but
  Bash-driven `git worktree add` (used by the `using-git-worktrees` skill)
  doesn't trigger that hook.
- Use `BTreeMap` (not `HashMap`) for iteration that affects PDF output (determinism)
- Blitz: `!important` reliability **unverified** (this note shares commit `7983fab5` with the since-disproven "Blitz not thread-safe" gotcha; no test/issue backing — measure before relying on or avoiding it), `padding-top` on inline roots ignored (use `margin-top`)
- `cargo fmt --check` enforced by CI
- **Coverage scope**: CI の coverage job は `cargo llvm-cov nextest --workspace --exclude fulgur-vrt --exclude fulgur-raikiri --exclude fulgur-dev`
  で動いている (`.github/workflows/ci.yml`)。Raikiri backend (`crates/fulgur-raikiri` / `crates/fulgur-dev`)
  は `.github/workflows/raikiri.yml` が別途 (関連 path の変更時と nightly のみ、非必須チェック) 回し、
  codecov へ `raikiri` flag 付き (carryforward) で upload する。`crates/fulgur-vrt` は別ジョブで実行されるため、
  **VRT reftest だけでカバーした draw 経路は codecov の patch coverage に乗らない**。
  新しい draw / convert / pageable ロジックを書くときは VRT に加えて lib 側にもテストを置く:
  - 純関数 (helper, fixup, math) → 当該モジュールの `#[cfg(test)] mod tests` に unit test
  - レンダリング経路 (`draw_background_layer` の match arm 等、`Engine::render` を通って初めて
    叩かれる箇所) → `crates/fulgur/tests/render_smoke.rs` に end-to-end smoke test
    (`Engine::builder().build().render(html)` で `assert!(!pdf.is_empty())`)
  VRT を後付けで足すと codecov に再指摘されて lib 側 smoke test を追加する手戻りが発生する
  (PR #244 で実例)。最初から両方書くこと。
- **`Engine` is a builder**: `Engine::builder().page_size(PageSize::A4).base_path(root).build()` + single-arg `render(html)`. `render_html(html)` still exists as a `#[deprecated(since = "0.19.0")]` alias — do not use it in new code. There is no `Engine::new().with_*()`.
- **VRT は PDF byte 比較**: `crates/fulgur-vrt` は HTML → PDF を生成して `goldens/fulgur/**/*.pdf` と byte-wise 比較する (`crates/fulgur-cli/tests/examples_determinism.rs` と同じ哲学)。pdftocairo は失敗時の diff 画像生成のみで使う。golden 更新は `FONTCONFIG_FILE="$PWD/examples/.fontconfig/fonts.conf" FULGUR_VRT_UPDATE=1 cargo test -p fulgur-vrt`。

---

## Fork notes (StudioMaak) — not for upstream PRs

This is the **StudioMaak fork**. Everything above is upstream's; this section is ours, and
should be stripped from any branch offered upstream.

Why we forked: paperworx renders a large segment of documents needing no JavaScript.
Measured **102 ms** for a real NBB jaarrekening inside a Worker isolate (fulgur-wasm in
workerd) against **1128 ms** through Chrome + Paged.js — no browser, no container. Full
Chrome stays for the rest. Four defects are written up in `paperworx-repros/`; the plan is
to fix each here and offer it upstream as a separate PR.

**Check the git history before believing "fulgur never implemented X".** Defect 2 was
written up as an unimplemented feature; `<thead>` repetition had in fact shipped in the v1
`Pageable` architecture (`TablePageable`, PR #14) and was dropped in the Phase 4 migration
to `Drawables`. The "not modelled in PR 5" / "deferred to a later change" comments were
the v2 authors recording a removal, not an absence. `git log --all -S <symbol>` found it in
seconds and turned the change from an invention into a port — with the original algorithm,
its example, and a review-fixed bug (`4d44c483`) to inherit.

**And check whether upstream is already doing it.** On 2026-08-24 that same defect 2
turned out to be under way upstream in PR #710, with two review rounds stacked on it
(#721, #728). Our port and theirs rendered the 300-row repro **byte-for-byte the
same**, but ours failed five of their reviewed edge-case probes — including
whitespace text nodes defeating the orphan check, which every pretty-printed table
hits. We dropped ours and took theirs; only `<tfoot>` repetition, which they decline
to implement, stayed ours. Cost of finding out late: a full implementation thrown
away. `gh pr list --repo fulgur-rs/fulgur` before starting a feature, not after.

### `children.len()` lies on any formatted document

Blitz keeps whitespace-only text nodes between elements, so indented markup gives
a node `[ws, div, ws, div, ws]`. Any code that reasons about "does this box have
one child or several", or measures "the leading child", is wrong on every
pretty-printed document and right on every minified one — which is exactly the
way round that makes it survive testing.

This is what made our `<thead>` port fail upstream's orphan probe: the leading
unit of the first body row measured a 0-height whitespace node, so the check that
stops a page carrying nothing but a repeated header never fired. Filter children
by `text_data()`-is-not-all-whitespace **and** `size.height > 0.0` before counting
them. Upstream's `repeating_table_header` has the reference version.

### Tracking upstream

`upstream/main` is mostly coverage and dependency traffic, but not *only* that — read
the source diff before assuming a sync is free. Measured 2026-09-11 over the 77 commits
since `dbf4dc62`: 15 changed library source, and among them was a **P0 silent
content-loss fix** (upstream PR #741) that a title-level skim reads as routine.

**The #719 cluster is frozen, not progressing.** Measured 2026-10-21: the draft is
still `CHANGES_REQUESTED`, and its author has not pushed to the repo since
**2026-09-09**. The extractions are #754, #755, #757 and — a fifth one earlier notes
missed — **#756** (WPT `css-break` conformance). None merged, none reviewed, none
closed. All five share an `updatedAt` of 2026-09-23, which is a repo-wide base-ref
event every stale PR in the repo carries, not author activity. `gh search prs --repo
fulgur-rs/fulgur "Extracted from #719"` returns exactly those five and nothing newer.
Every PR opened since 2026-09-20 is coverage, dependabot or Raikiri, by a different
author.

**Decision (unchanged, now firmer): do not wait on any of them.** Offer repros 11 and
12 as standalone PRs, each with the WeasyPrint/Chrome measurement already in
`paperworx-repros/README.md`. If #719 ever lands, reconciling a merged small PR is
upstream's problem, not ours.

**A sync's value is not in its feature list.** Measured 2026-10-21 over the 331
commits since `c89bb3b3`: upstream made **zero** commits to `pagination_layout.rs`,
`paginate.rs`, `render.rs` or `gcpm/`, and all four held defects (8, 11, 12, 14) were
verified untouched by symbol search. No features we wanted. The sync was still worth
taking, because buried in the coverage traffic was a **live arbitrary-file-read with
exfiltration into the output PDF** — see below. Twice now (PR #741's content-loss fix,
and this) the reason to sync has been a defect fix whose commit title reads as
routine. Read the source diff; do not grade a sync by its changelog.

Three mechanical traps these syncs exposed, all worth repeating next time:

- **`git merge` reported one conflict and silently merged two more files wrong.**
  `paragraph.rs` (upstream's new `make_test_inline_box` predates our three
  `InlineBoxItem` fields) and a probe fn that landed twice verbatim. Neither shows in
  `git status`; `cargo build` catches only one. **Gate a sync on
  `cargo clippy --all-targets`, not on a clean merge.**
- **One conflict is a standing policy disagreement, not a mechanical one.** Upstream's
  `fragment_block_subtree_break_before_after_gap_places_child_at_y_zero` asserts a
  forced `break-before: page` discards the child's top margin; our `874877ef` /
  `8d75e147` keep it, measured against WeasyPrint 69 and Chrome 151. Upstream is
  internally inconsistent — its own
  `body_level_break_before_preserves_own_top_margin_on_new_page` asserts the margin
  *is* kept at body level. Expect this to re-conflict on every sync until it is
  settled upstream. (It did **not** re-conflict on 2026-10-21 — only because upstream
  never entered `pagination_layout.rs` that window. The disagreement is dormant, not
  resolved.)
- **A clean auto-merge can still delete one of our tests.** On 2026-10-21 upstream's
  `87bfffb2` inserted new SVG tests exactly where our
  `build_static_content_css_serializes_selectors_and_escapes` lived and deleted it.
  Git took our modified function body *and* upstream's deletion of its test: no
  conflict, no compile error, nothing in `git status`, clippy green, suite green —
  because the only thing lost was coverage. Nothing automated catches this.
  **After a sync, diff the test inventory, not just the build:**
  `git diff <premerge>..HEAD -- '*.rs' | grep "^-.*fn .*("` and read what left.

#### Upstream sync, 2026-11-11

99 commits since `efb88937`, merged at `bd6222fe`. The Blitz library diff is nine
files, and eight are test-only. The one behaviour change is **`bd59747c`
(breaking): `@page` margin boxes resolve `counter()` / `counters()` for `page` and
`pages` only, and every other name to 0** — matching Chromium and Raikiri; the
documented route to a chapter number in a header is `string-set` + `string()`.
paperworx templates use no document counters in margin boxes (searched), and none
of our GCPM patches touch `gcpm/counter.rs`, so nothing of ours moved. Blast radius
over 86 fixtures and examples: only `gcpm/counter-page-numbers`; vol-kap
byte-identical.

What it was actually for: the pinned `fonts.conf` stopped including the system
config and the Blitz goldens were re-cut on bundled Noto (see *Baselines*).
Upstream also now **publishes `fulgur-raikiri`** (`deed6d26`); the "walled off"
section below predates that.

Trap hit: every binary conflict (22 goldens, 3 example PDFs) was a fixture the fork
had already re-blessed for its own fixes. Taking upstream's copy makes VRT fail on
exactly those 22; regenerate them in the container (`FULGUR_VRT_UPDATE=failing`)
rather than keeping ours, which carry the old fonts. The forced-break margin policy
did not re-conflict: upstream touched `pagination_layout.rs` only in a doc comment.

#### usvg reads local files, and our base-path confinement never sees it

Closed upstream 2026-10-21 (`0b167cad`, `234ee3d3`, `80b129f6`, `87bfffb2`,
`c8b0d1e4`); recorded here because the shape of the bug outlives the fix.

`usvg::Options::default()` installs a resolver that treats any `<image href>` it
cannot parse as a `data:` URL as a **filesystem path**, and with `resources_dir`
unset — we never set it — an absolute path is used verbatim. The read happens
*inside usvg*, so the `canonical_base` check in `net.rs` is not on the path and
cannot help. The bytes are then painted into the output PDF, so this is
read-and-exfiltrate, not a blind read. Demonstrated against our own tree: a document
confined to a base directory read a file outside it and rendered its contents, plain
CLI, no flags. Four entry paths were unguarded — inline `<svg>`, `<img src>` / any
net-provider payload, CSS `background-image: url()`, and list markers.

Three things to carry forward:

- **A regex over the payload is not a fix.** The `<image>` can arrive inside escaped
  text or CDATA (Blitz re-serializes with `outer_html()`, which writes text verbatim,
  so escaped markup becomes real markup on the second pass), inside gzip (SVGZ), or
  behind a leading comment or DOCTYPE that defeats extension and magic sniffing.
  Upstream's answer is scan → cut → re-scan → reject if still dirty.
- **Blitz hands usvg anything it cannot decode as a raster**, whatever the extension.
  A `.css` or extensionless `<link>` target is an SVG parse vector.
- **The Worker path is immune by accident.** On `wasm32-unknown-unknown` `std::fs`
  returns `Unsupported`, so `path.exists()` is false and usvg bails before reading.
  No `cfg` in the fix is what closes it — a move to `wasm32-wasip1`/`wasip2` reopens
  it instantly. And the SVGZ decompression bomb is target-independent, so Workers are
  immune to the disclosure but **exposed to the DoS**, where memory exhaustion hits a
  hard isolate cap.

#### Raikiri is a second backend, walled off from us

Landed 2026-09-27 onward: `crates/fulgur-raikiri` (a complete independent layout
backend, Raikiri for layout + Krilla for paint, no Blitz fallback) and
`crates/fulgur-dev` (an unpublished CLI that renders through either backend for
comparison).

**Since `deed6d26` (taken in the 2026-11-11 sync) `fulgur-raikiri` is published.** It
depends on the crates.io `raikiri-*` 0.0.6 crates and is released in the same version
group as `fulgur-core` / `fulgur-blitz`. `fulgur-dev` stays `publish = false`. What has
*not* changed for us:

- The `fulgur` facade and `fulgur-cli` still do not depend on it. Only `fulgur-dev`,
  and `fulgur-vrt` / `fulgur-wpt` behind an optional `raikiri` feature, pull it in.
  `cargo test -p fulgur` does not build it.
- CI still excludes both crates from fmt, clippy, nextest and coverage in `ci.yml`.
  They run in the separate, non-required `raikiri.yml`.

So publishing makes Raikiri a real release artefact upstream, and a signal of where
upstream is heading. It is not a dependency of anything we ship.

It already beats Blitz on paged media — 116 WPT `css-page` passes against Blitz's 84,
with 50 passing only on Raikiri — so expect upstream's attention to keep moving there.
Multicol is still weak.

One structural consequence for us: tagged-PDF classification moved **out of
`fulgur-blitz` into `fulgur-core`** (`fulgur-core/src/tagging.rs`, new) so both
backends share it. `PdfTag`, `SemanticEntry` and `classify_element` now live there,
re-exported from `fulgur-blitz::tagging` via a glob, so source compatibility holds —
but a fork patch against those definitions must now target `fulgur-core`.

#### Dependency PRs that would undercut our measurements

Open and unmerged as of 2026-10-21, both of which land directly under fixes of ours:

- **#747 parley 0.6 → 0.11.1.** Repro 10's fix exists *because* parley 0.6 has no
  strut and no `vertical-align` (`extract_strut_style` / `resolve_strut` /
  `align_inline_boxes`). Five minor versions could make it redundant, wrong, or both.
- **#748 taffy 0.9.2 → 0.14.0.** Repro 12's whole premise is that Taffy resolves
  absolute insets against the immediate parent.

Re-measure both repros against the real references before merging either bump.

Paged.js is not a design reference for pagination features: 0.4.3's only table-aware code
propagates `break-inside: avoid` from `<tbody>`/`<thead>`, and it has no repeat concept at
all. Because it replaces Chrome's own pagination with DOM chunking, Chrome's native header
repetition never engages either — so the Chrome + Paged.js path we ship today does not
repeat theads. WeasyPrint's `layout/table.py` is the reference worth reading.

### Baselines

`cargo test -p fulgur --lib` was **2014 passed / 0 failed** at fork point (`682bcbf3`).
The "~340 unit tests" figure in Common Commands above is long stale. `cargo test -p fulgur`
adds ~30 integration binaries — 2468 passing at the fork point, 2585 as of repros
10-12, and 2618 / 0 failed / 7 ignored after adopting upstream's repeating-header
stack (its probe suites come with it).

Since the `fulgur` → `fulgur-core` + `fulgur-blitz` split, count all three together;
`-p fulgur` alone now measures only the facade's integration tests and badly
understates the suite:

| revision | `-p fulgur -p fulgur-blitz -p fulgur-core` | `-p fulgur-cli` |
|---|---|---|
| after the GCPM work (2026-09-11) | 2753 / 0 / 10 | 57 / 0 |
| after the 2026-10-21 upstream sync | 3003 / 0 / 10 (67 binaries) | 59 / 0 |
| pagination fixes + break matrix (`28db2b60`) | 3008 / 0 / 10 (69 binaries) | 59 / 0 |
| **after the 2026-11-11 upstream sync** | **3035 / 0 / 10** (69 binaries) | **59 / 0** |

**`examples_determinism` does not validate the committed example PDFs.** Its 10 tests
render each example *twice* and compare the two runs to each other, so they prove
determinism and nothing more — a committed `examples/**/index.pdf` that no longer
matches the code passes this suite happily. After a sync, regenerate the examples in
the pinned container and `git status` them; that diff is the only thing that tells you
which committed PDFs your code still reproduces. (Measured 2026-10-21: 15 of 20 upstream
regenerations reproduced byte-exactly under our code, 5 did not.)

**`fulgur-vrt` needs `ubuntu:24.04` with `fonts-dejavu-core` 2.37-8** — that is the only
environment its byte-exact goldens reproduce in. Measured:

| environment | DejaVu | result at `682bcbf3` |
|---|---|---|
| macOS host | resolves *Helvetica* | 29 of 64 fail |
| `debian:bookworm` arm64 | 2.37-**6** | 29 fail, but within 0-36 bytes |
| **`ubuntu:24.04` arm64** | 2.37-**8** | **64/64 pass, byte-identical** |

Two things follow. **Architecture is irrelevant** — arm64 reproduces goldens cut on x86_64
CI exactly, so fulgur's PDF output really is arch-independent and only the font floats.
And the requirement is that *specific font package revision*: Debian's 2.37-6 and Ubuntu's
2.37-8 are the same upstream version with different bytes, which is the whole 0-36 byte
residual.

`FONTCONFIG_FILE` does not save you here — it is a **proven no-op on macOS** (rendering is
byte-identical with and without it), because fontconfig is not macOS's font mechanism.
`gcpm_snapshot.rs` shows the robust alternative: it injects Noto via
`AssetBundle::add_font_file`, and its 19 byte-exact goldens pass unmodified on macOS.

**Since the 2026-11-11 sync the goldens embed the bundled Noto set, not DejaVu.**
Upstream's `b9537fb2` dropped the `/etc/fonts` include from `fonts.conf`, so only
`examples/.fonts` is visible, and `75e5f70d` re-cut the goldens on it (verified with
`pdffonts`: NotoSans / NotoSansMono only). The DejaVu package-revision table above is
therefore history — the runner image's default font should no longer reach the
output. Unverified whether `debian:bookworm` now reproduces too; keep using the
`ubuntu:24.04` recipe until someone measures it. `FONTCONFIG_FILE` must be absolute.

**The Noto pin does not make VRT pass on macOS.** Measured 2026-11-11 at `2c4565cb`
with an absolute `FONTCONFIG_FILE`: **30 of 65 fixtures differ**. That is the same
no-op as above. macOS does not resolve fonts through fontconfig, so the pin never
reaches it. Container-only still stands.

To run VRT (~2 min, mostly the build):

```bash
docker volume create vrt-target
docker run --rm -v "$PWD":/work -v vrt-target:/tmp/lt -w /work \
  -e CARGO_TARGET_DIR=/tmp/lt ubuntu:24.04 bash -c '
    export DEBIAN_FRONTEND=noninteractive
    apt-get update -qq && apt-get install -y -qq fonts-dejavu-core fontconfig \
      poppler-utils curl build-essential pkg-config python3
    curl -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain stable
    export PATH=/root/.cargo/bin:$PATH
    export CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0
    export FONTCONFIG_FILE=/work/examples/.fontconfig/fonts.conf
    cargo test -p fulgur-vrt --test vrt_test -j 4'
```

`python3` is required (stylo's build script) and `poppler-utils` only on the failure path.
Keep `-j 4` and `debug=0`: ten parallel linkers with full debug info OOM-kill `ld` in
Docker's 8 GB VM.

**Do not gate on VRT from macOS.** It cannot see your change — 17 of its 29 local failures
are font noise, and real regressions hide behind them.

**Enumerate the goldens a change moves without running VRT**, by rendering every manifest
fixture through the CLI with and without the change and diffing bytes. That works on
macOS, it costs one build, and it is what lets parallel work report its blast radius
before anyone regenerates anything. Repros 10-12 predicted 17 moving fixtures this way and
the container named exactly those 17.

### Pagination ground rules: spec and reference first, then code

Set by Bert on 2026-11-12, after a paragraph-splitting fix derived from probes
alone lost lines in padded paragraphs. For any change to pagination
(`pagination_layout.rs`, the paragraph and block slicing in `render.rs`), in
this order:

1. **Spec.** Quote the text that governs the case: css-break-3 / css-break-4,
   css-tables-3, css-page-3.
2. **Reference implementation.** Find the Chromium LayoutNG code that
   implements it and how it reasons: `third_party/blink/renderer/core/layout/`
   — `block_layout_algorithm.cc`, `fragmentation_utils.cc`, `break_appeal.h`,
   `block_break_token.h`, `table/`, and `block_fragmentation_tutorial.md`.
   Where Chrome departs from the spec, say so and say which one we follow.
3. **Model.** Ask what the walker has to know or remember to follow the rule.
   If it cannot hold that (a fragment's content, a better earlier break, a
   page's own height), widen the walker first. Do not fit the case into the
   existing walk with another local check.
4. **Show Bert steps 1–3** before writing code.
5. **Code, then measure.** Probes against Chrome and Prince verify the rule;
   they are not where the rule comes from. A probe set built from the cases
   you thought of will pass; the padded and inline-block paragraphs were found
   by an independent reviewer, not by the 110 probes that all matched.

The current model, its gaps and the planned order are in
`docs/plans/2026-11-11-pagination-walker-box-list.md`, section "Fragmentation
model".

### Never trust a README over a measurement

WeasyPrint 69 and Chrome 151 headless-shell **agree to ~0.1 mm** on every case tested, so
together they are the reference. This has caught four defects, killed one plausible
hypothesis, and corrected two confident-but-wrong claims.

```bash
uvx --from weasyprint weasyprint in.html out-weasy.pdf     # reference A
cargo run -p fulgur-cli -- render -o out-ful.pdf in.html   # subject
pdftotext -bbox -f 1 -l 1 out.pdf -                        # xMin/yMin in pt; × 25.4/72 = mm
mutool draw -F stext -o - -i out.pdf 1                     # baselines — use this for vertical
```

**Compare baselines, not `pdftotext -bbox`, for anything vertical.** `yMin` is
*baseline − ascent*, and the writers disagree about the ascent: Chrome declares hhea
(Liberation Sans 0.905em), krilla declares OS/2 typo (0.728em). That is a **constant
+0.56mm on every fulgur `yMin` at 9pt**, present where the engines agree exactly, and it
inflated a reported page-1 error series from −0.09/+0.18/+1.76/+3.35mm to
+0.50/+0.88/+2.33/+3.91 — enough to look like a ±1% failure that was not there. `mutool
draw -F stext` reports the glyph origin (`y=`) and the font size directly, so nothing
depends on either font descriptor. `-bbox` is still right for horizontal work.

**A monotonic-looking ramp built by zipping two word lists is not evidence of
accumulation.** Zipping misaligns as soon as pagination diverges, and averages localized
errors of different sizes into a trend. Match like for like — same page, same word — and
check whether the series *returns*: paperworx repro 9's did, on every third line, which
is what disproved a per-line-advance hypothesis and turned one "cumulative drift" into
four independent defects.

Chrome 151: `~/.cache/puppeteer/chrome-headless-shell/mac_arm-151.0.7922.71/` via
`puppeteer-core`, `headless: 'shell'`, viewport 1920×1080. The driver script must sit in a
directory that resolves `puppeteer-core` — a bare ESM import resolves against the script's
own path, not the cwd.

**Isolate one variable per file.** A margin box alone on its edge is given that edge's
whole band, so where the glyphs land names its alignment directly, with none of the width
distribution mixed in. Sixteen one-slot files beat one sixteen-slot file.

**Confirm output is non-blank before believing a timing.** A 7 ms render of an empty page
benchmarks beautifully.

### Two traps when reading PDF output back

- **`inspect`'s `width` is a crude `chars × font_size` estimate** — 32pt for a run that
  measures 24pt. Never assert on it or derive a right edge from it. Use `pdftotext -bbox`
  for real ink extents, or write assertions that need no width at all.
- **`inspect` cannot recover text**: lopdf 0.40 does not read krilla's `ToUnicode` CMap, so
  strings come back as raw glyph ids. Position is sound; content is not.

**Prefer anchor invariants to absolute coordinates.** Absolute positions bake in the
reference engine's font. Assert relations that survive any font: a left-aligned box's
`xMin` equals its band's left edge; a centred box's midpoint equals the band centre; three
boxes sharing a rect satisfy `right − left == 2 × (centre − left)` exactly, the glyph width
cancelling out. See `crates/fulgur/tests/margin_box_alignment.rs`.

### Where margin-box layout responsibility splits

Know this before touching `gcpm/` or `render.rs` — a fix on the wrong side of the line
looks fine and is wrong.

- **fulgur owns the box's rect.** `gcpm/margin_box.rs::compute_edge_layout` implements the
  CSS Paged Media 3 §5.3.3 distribution in Rust, fed by intrinsic sizes the engine measured.
- **Blitz owns everything inside the rect.** `render_page` hands it a document whose
  viewport *is* the rect.

So per-box presentation belongs in that handed-over document (`margin_box_document`), as
ordinary CSS through the normal cascade — **not** as an adjustment to the painted result.
Translating the paint origin would drag the box's background and borders along with its
content when only the content is meant to move.

`gcpm/ua_css.rs` is *not* the seam: it is GCPM-only (`bookmark-level` and friends, parsed
by fulgur's own parser) and by its own doc comment "never reaches Blitz".

Measured Blitz/Taffy behaviour behind that choice:

- `display:flex; flex-direction:column; justify-content:center|flex-start|flex-end`
  resolves **exactly** (verified to 0.05pt).
- `display:table-cell` + `vertical-align:middle` **silently does nothing** — content stays
  at the box's top edge.
- Content taller than its box degrades to top-aligned and stays inside the box; the
  pagination pass fragments it and only page 0 is drawn. It does not spill upward.

Precedence inside a box: the wrapper's inline style carries fulgur's defaults, while an
author's at-rule declarations render on an **inner** element — and an inline style beats an
inherited value, so the author still wins. The zeroed `margin`/`padding` already rely on
this.

### The margin box's box model is not settled — and we only match Chrome on one axis

Measured with a red `background` on `@bottom-center`, A4/25mm (band =
x 25-185mm, y 272-297mm):

| engine | horizontal | vertical |
|---|---|---|
| WeasyPrint 69 | 103.0-105.8mm (shrink-wrapped to the text) | 272.3-296.3mm (full band) |
| fulgur | 25.4-184.1mm (full width) | 282.9-285.8mm (content height only) |
| Chrome 151 | 24.7-184.1mm (full width) | 272.3-296.3mm (full band) |

Chrome's box *is* the rect on both axes, which is what §5.3.3 implies;
WeasyPrint and fulgur are transposed versions of it. This is the one place
the two references disagree with each other, so "they agree, therefore
reference" does not settle it — Chrome's reading does.

It explains the `text-align` divergence too: WeasyPrint shrink-wraps
horizontally, so alignment inside the box is moot and an author's
`text-align` looks ignored. fulgur is full-width there and matches Chrome.

The open gap is vertical: a margin box's background does not fill its band,
because author `declarations` render on an inner element rather than on the
box. Moving them to the wrapper would fix it — but the wrapper zeroes
`margin`/`padding` precisely so the renderer can paint at `rect.x, rect.y`
with a (0, 0) body offset, so an author `margin` would need handling first.

### Known noise

`ERROR: Unexpected token` on stderr during renders comes from the upstream CSS parser
meeting the nested `@page { @bottom-right { … } }` at-rules. Pre-existing and cosmetic —
don't chase it while debugging something else.

### The WASM/Worker path

```bash
wasm-pack build crates/fulgur-wasm --target web --release   # speed build
mise run wasm-build                                         # -Oz size build (~7 MB)
```

Speed build: 10.8 MB raw → 3.9 MB gzip → **2.6 MB brotli**. Worker limits are 3 MB free /
10 MB paid compressed, so it fits. Runs under `workerd serve` with wasm/font/fixture
embedded as capnp modules; `initSync({module})` at module scope costs ~1 ms per isolate.

**WASM has no system fonts**, so registering `theme.assets[]` fonts is mandatory — see
defect 3, where a font miss yields a blank document of record and still reports success.
