//! Optional once-per-second browser and UI performance counters.

use std::{
    fs,
    sync::{
        OnceLock,
        atomic::{AtomicU64, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

static ENABLED: OnceLock<bool> = OnceLock::new();
static REPORTER: OnceLock<()> = OnceLock::new();
static OSR_PAINTS: AtomicU64 = AtomicU64::new(0);
static UI_RENDERS: AtomicU64 = AtomicU64::new(0);
static FRAME_COPY_NANOS: AtomicU64 = AtomicU64::new(0);
static FRAME_COPIES: AtomicU64 = AtomicU64::new(0);

fn enabled() -> bool {
    *ENABLED.get_or_init(|| std::env::var_os("LUMEN_STATS").is_some_and(|value| value == "1"))
}

fn start_reporter() {
    REPORTER.get_or_init(|| {
        if let Err(error) = thread::Builder::new()
            .name("lumen-stats".into())
            .spawn(|| loop {
                thread::sleep(Duration::from_secs(1));
                let paints = OSR_PAINTS.swap(0, Ordering::Relaxed);
                let renders = UI_RENDERS.swap(0, Ordering::Relaxed);
                let copy_nanos = FRAME_COPY_NANOS.swap(0, Ordering::Relaxed);
                let copies = FRAME_COPIES.swap(0, Ordering::Relaxed);
                let avg_copy_ms = if copies == 0 {
                    0.0
                } else {
                    copy_nanos as f64 / copies as f64 / 1_000_000.0
                };
                eprintln!(
                    "Lumen stats: OSR paints/s={paints} root UI renders/s={renders} avg frame-copy={avg_copy_ms:.3} ms RSS={}",
                    process_rss()
                );
            })
        {
            eprintln!("Lumen stats reporter failed to start: {error}");
        }
    });
}

pub fn frame_copy_started() -> Option<Instant> {
    enabled().then(|| {
        start_reporter();
        Instant::now()
    })
}

pub fn record_osr_paint(copy_started: Option<Instant>) {
    if !enabled() {
        return;
    }
    OSR_PAINTS.fetch_add(1, Ordering::Relaxed);
    if let Some(started) = copy_started {
        FRAME_COPY_NANOS.fetch_add(started.elapsed().as_nanos() as u64, Ordering::Relaxed);
        FRAME_COPIES.fetch_add(1, Ordering::Relaxed);
    }
}

pub fn record_ui_render() {
    if !enabled() {
        return;
    }
    start_reporter();
    UI_RENDERS.fetch_add(1, Ordering::Relaxed);
}

fn process_rss() -> String {
    fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|status| {
            status.lines().find_map(|line| {
                line.strip_prefix("VmRSS:")
                    .map(|value| value.trim().to_string())
            })
        })
        .unwrap_or_else(|| "unavailable".into())
}
