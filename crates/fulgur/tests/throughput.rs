//! Per-request cost probe — what one single-threaded worker sustains.
//!
//! A Cloudflare Worker isolate is single-threaded, so `render_batch`'s rayon
//! parallelism does not apply there: concurrency comes from many isolates, and
//! per-isolate throughput is just 1 / per-request latency. This measures
//! independent renders, not a batch, and separates the two costs a request can
//! carry:
//!
//!   - shared engine  — fonts parsed once at module scope, as a Worker would
//!   - fresh engine   — the full cost if setup were paid per request
//!
//! Run: cargo test --release -p fulgur --test throughput -- --ignored --nocapture
//! Env: FULGUR_BENCH_HTML (document), FULGUR_BENCH_N (iterations)
use fulgur::Engine;
use fulgur::asset::AssetBundle;
use std::time::{Duration, Instant};

fn assets() -> AssetBundle {
    let mut a = AssetBundle::default();
    let fd = std::env::var("FULGUR_BENCH_FONTS")
        .unwrap_or_else(|_| "/tmp/liberation-fonts-ttf-2.1.5".into());
    for f in [
        "LiberationSans-Regular.ttf",
        "LiberationSans-Bold.ttf",
        "LiberationSans-Italic.ttf",
        "LiberationSans-BoldItalic.ttf",
    ] {
        let _ = a.add_font_file(format!("{fd}/{f}"));
    }
    a
}

fn synthetic() -> String {
    let body: String = (0..400)
        .map(|i| format!("<p>Paragraph {i}: the quick brown fox jumps over the lazy dog, repeatedly and at length.</p>"))
        .collect();
    format!("<!doctype html><html><head><style>@page{{size:A4;margin:20mm}}\
             body{{font-family:'Liberation Sans';font-size:10pt}}</style></head><body>{body}</body></html>")
}

fn stats(mut v: Vec<Duration>) -> (Duration, Duration, Duration) {
    v.sort();
    (v[v.len() / 2], v[v.len() * 95 / 100], v[v.len() - 1])
}

#[test]
#[ignore = "benchmark; run explicitly in release"]
fn per_request_cost() {
    let html = match std::env::var("FULGUR_BENCH_HTML") {
        Ok(p) => std::fs::read_to_string(&p).expect("read bench html"),
        Err(_) => synthetic(),
    };
    let n: usize = std::env::var("FULGUR_BENCH_N").ok().and_then(|v| v.parse().ok()).unwrap_or(20);

    // Shared engine: fonts parsed once, as a Worker would at module scope.
    let engine = Engine::builder().assets(assets()).build();
    let pdf = engine.render(&html).expect("warm-up render");
    let mut shared = Vec::new();
    for _ in 0..n {
        let t = Instant::now();
        let out = engine.render(&html).expect("render");
        shared.push(t.elapsed());
        std::hint::black_box(out);
    }

    // Fresh engine each time: the cost if setup were paid per request.
    let mut fresh = Vec::new();
    for _ in 0..n.min(10) {
        let t = Instant::now();
        let e = Engine::builder().assets(assets()).build();
        let out = e.render(&html).expect("render");
        fresh.push(t.elapsed());
        std::hint::black_box(out);
    }

    let (s50, s95, smax) = stats(shared);
    let (f50, _, _) = stats(fresh);
    println!("\ninput {:.0} KB -> pdf {:.0} KB, {n} iterations, single-threaded",
        html.len() as f64 / 1024.0, pdf.len() as f64 / 1024.0);
    println!("  shared engine : p50 {s50:?}  p95 {s95:?}  max {smax:?}");
    println!("  fresh engine  : p50 {f50:?}   (setup delta {:?})", f50.saturating_sub(s50));
    println!("  => {:.1} requests/sec per single-threaded worker (p50, shared)",
        1.0 / s50.as_secs_f64());
}
