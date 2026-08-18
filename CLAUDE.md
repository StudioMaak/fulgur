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
cargo test -p fulgur --lib         # fulgur unit tests (~340)
cargo test -p fulgur
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

- `crates/fulgur/` — Library crate with the conversion engine
- `crates/fulgur-cli/` — CLI binary using clap

### Key Modules (fulgur)

- **engine.rs** — `Engine` builder: configures and executes `render()`
- **blitz_adapter.rs** — Thin adapter isolating Blitz API changes from the rest of the codebase
- **convert.rs** — Transforms Blitz DOM nodes into `Drawables` and geometry records
- **draw_primitives.rs** — Primitive geometry, canvas, style, and drawing-helper types (`Size`, `Rect`, `Canvas`, `BlockStyle`, background/gradient types, etc.)
- **drawables.rs** — `Drawables` struct: flat per-node draw payloads (`BlockDraw`, `ParagraphDraw`, `ImageDraw`, etc.) used by the v2 render path
- **paginate.rs** — Page splitting algorithm that walks the `PaginationGeometryTable`
- **render.rs** — Draws paginated fragments onto Krilla surfaces via `render_v2`
- **config.rs** — Page size, margins, orientation, metadata
- **asset.rs** — `AssetBundle` manages CSS, fonts, and images (offline-first, all assets explicitly registered)
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
- **Coverage scope**: CI の coverage job は `cargo llvm-cov nextest --workspace --exclude fulgur-vrt`
  で動いている (`.github/workflows/ci.yml`)。`crates/fulgur-vrt` は別ジョブで実行されるため、
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

Paged.js is not a design reference for pagination features: 0.4.3's only table-aware code
propagates `break-inside: avoid` from `<tbody>`/`<thead>`, and it has no repeat concept at
all. Because it replaces Chrome's own pagination with DOM chunking, Chrome's native header
repetition never engages either — so the Chrome + Paged.js path we ship today does not
repeat theads. WeasyPrint's `layout/table.py` is the reference worth reading.

### Baselines

`cargo test -p fulgur --lib` was **2014 passed / 0 failed** at fork point (`682bcbf3`).
The "~340 unit tests" figure in Common Commands above is long stale. `cargo test -p fulgur`
adds ~30 integration binaries — 2468 passing at the fork point, **2585 / 0 failed / 4
ignored** as of repros 10-12.

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
Note also that the goldens embed **DejaVuSans**, not the bundled Noto Sans the pinned
`fonts.conf` asks for, so VRT's determinism actually rests on the runner image's default
font rather than on the bundled set. A runner image bump would break every text fixture at
once. `gcpm_snapshot.rs` shows the robust alternative: it injects Noto via
`AssetBundle::add_font_file`, and its 19 byte-exact goldens pass unmodified on macOS.

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
