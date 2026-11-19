//! Optional once-per-second browser and UI performance counters.

use std::{
    collections::HashMap,
    fs,
    io::Write as _,
    sync::{
        Mutex, OnceLock,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

static PLUGIN_ENABLED: AtomicBool = AtomicBool::new(false);
static REPORTER: OnceLock<()> = OnceLock::new();
static OSR_PAINTS: AtomicU64 = AtomicU64::new(0);
static UI_RENDERS: AtomicU64 = AtomicU64::new(0);
static ROOT_METADATA_UPDATES: AtomicU64 = AtomicU64::new(0);
static FRAME_COPY_NANOS: AtomicU64 = AtomicU64::new(0);
static FRAME_COPIES: AtomicU64 = AtomicU64::new(0);
static LOOP_UPDATE_NANOS: AtomicU64 = AtomicU64::new(0);
static LOOP_WAIT_NANOS: AtomicU64 = AtomicU64::new(0);
static LOOP_ITERS: AtomicU64 = AtomicU64::new(0);
static PAINT_LATENCY_NANOS: AtomicU64 = AtomicU64::new(0);
static PAINT_LATENCIES: AtomicU64 = AtomicU64::new(0);
static CEF_PAINT_NANOS: AtomicU64 = AtomicU64::new(0);
static CEF_PAINTS: AtomicU64 = AtomicU64::new(0);
static SCROLL_CEF_PAINTS: AtomicU64 = AtomicU64::new(0);
static SCROLL_CEF_COALESCED: AtomicU64 = AtomicU64::new(0);
static IMAGE_COPY_NANOS: AtomicU64 = AtomicU64::new(0);
static IMAGE_COPIES: AtomicU64 = AtomicU64::new(0);
static VIEWPORT_RENDER_NANOS: AtomicU64 = AtomicU64::new(0);
static VIEWPORT_RENDERS: AtomicU64 = AtomicU64::new(0);
static BLOCKED_BY_TAB: OnceLock<Mutex<HashMap<String, u64>>> = OnceLock::new();

fn enabled() -> bool {
    PLUGIN_ENABLED.load(Ordering::Relaxed)
        && std::env::var_os("ANARC_STATS").is_some_and(|value| value == "1")
}

pub(crate) fn set_plugin_enabled(enabled: bool) {
    PLUGIN_ENABLED.store(enabled, Ordering::Relaxed);
}

#[cfg(test)]
pub(crate) fn plugin_enabled() -> bool {
    PLUGIN_ENABLED.load(Ordering::Relaxed)
}

fn start_reporter() {
    REPORTER.get_or_init(|| {
        if let Err(error) = thread::Builder::new()
            .name("lumen-stats".into())
            .spawn(|| loop {
                thread::sleep(Duration::from_secs(1));
                let paints = OSR_PAINTS.swap(0, Ordering::Relaxed);
                let renders = UI_RENDERS.swap(0, Ordering::Relaxed);
                let metadata = ROOT_METADATA_UPDATES.swap(0, Ordering::Relaxed);
                let copy_nanos = FRAME_COPY_NANOS.swap(0, Ordering::Relaxed);
                let copies = FRAME_COPIES.swap(0, Ordering::Relaxed);
                let loop_update = LOOP_UPDATE_NANOS.swap(0, Ordering::Relaxed);
                let loop_wait = LOOP_WAIT_NANOS.swap(0, Ordering::Relaxed);
                let loop_iters = LOOP_ITERS.swap(0, Ordering::Relaxed).max(1);
                let paint_latency = PAINT_LATENCY_NANOS.swap(0, Ordering::Relaxed);
                let paint_latencies = PAINT_LATENCIES.swap(0, Ordering::Relaxed).max(1);
                let cef_paint = CEF_PAINT_NANOS.swap(0, Ordering::Relaxed);
                let cef_paints = CEF_PAINTS.swap(0, Ordering::Relaxed).max(1);
                let image_copy = IMAGE_COPY_NANOS.swap(0, Ordering::Relaxed);
                let image_copies = IMAGE_COPIES.swap(0, Ordering::Relaxed).max(1);
                let viewport_render = VIEWPORT_RENDER_NANOS.swap(0, Ordering::Relaxed);
                let viewport_renders = VIEWPORT_RENDERS.swap(0, Ordering::Relaxed).max(1);
                let blocked = BLOCKED_BY_TAB
                    .get_or_init(|| Mutex::new(HashMap::new()))
                    .lock()
                    .map(|counts| counts.iter().map(|(tab, count)| format!("{tab}:{count}")).collect::<Vec<_>>().join(","))
                    .unwrap_or_default();
                let avg_copy_ms = if copies == 0 {
                    0.0
                } else {
                    copy_nanos as f64 / copies as f64 / 1_000_000.0
                };
                eprintln!(
                    "Lumen stats: OSR paints/s={paints} root UI renders/s={renders} root metadata updates/s={metadata} avg frame-copy={avg_copy_ms:.3} ms CEF-on-paint={:.2} ms image-copy={:.2} ms viewport-render={:.2} ms loop-update={:.2} ms loop-wait={:.2} ms iters={loop_iters} paint-latency={:.2} ms blocked-by-tab=[{blocked}] RSS={}",
                    cef_paint as f64 / cef_paints as f64 / 1_000_000.0,
                    image_copy as f64 / image_copies as f64 / 1_000_000.0,
                    viewport_render as f64 / viewport_renders as f64 / 1_000_000.0,
                    loop_update as f64 / loop_iters as f64 / 1_000_000.0,
                    loop_wait as f64 / loop_iters as f64 / 1_000_000.0,
                    paint_latency as f64 / paint_latencies as f64 / 1_000_000.0,
                    process_rss()
                );
            })
        {
            eprintln!("Lumen stats reporter failed to start: {error}");
        }
    });
}

pub fn record_blocked_request(tab_id: &str) {
    if !enabled() {
        return;
    }
    let mut counts = BLOCKED_BY_TAB
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .unwrap();
    if counts.len() >= 4096 {
        counts.clear();
    }
    if let Some(count) = counts.get_mut(tab_id) {
        *count += 1;
    } else {
        counts.insert(tab_id.to_owned(), 1);
    }
}

#[allow(dead_code)]
pub fn record_osr_paint() {
    if !enabled() {
        return;
    }
    OSR_PAINTS.fetch_add(1, Ordering::Relaxed);
}

pub fn record_ui_render() {
    if !enabled() {
        return;
    }
    start_reporter();
    UI_RENDERS.fetch_add(1, Ordering::Relaxed);
}

/// TEMPORARY probe: split each UI update loop iteration into the synchronous
/// update work and the wait that follows it.
pub fn record_ui_loop(update: Duration, wait: Duration) {
    LOOP_UPDATE_NANOS.fetch_add(update.as_nanos() as u64, Ordering::Relaxed);
    LOOP_WAIT_NANOS.fetch_add(wait.as_nanos() as u64, Ordering::Relaxed);
    LOOP_ITERS.fetch_add(1, Ordering::Relaxed);
}

/// Scroll stage timings are aggregated into the regular scroll log, independent
/// of the optional stats plugin.
pub fn record_cef_paint(elapsed: Duration) {
    CEF_PAINT_NANOS.fetch_add(elapsed.as_nanos() as u64, Ordering::Relaxed);
    CEF_PAINTS.fetch_add(1, Ordering::Relaxed);
    SCROLL_CEF_PAINTS.fetch_add(1, Ordering::Relaxed);
}

pub fn record_coalesced_cef_paint() {
    SCROLL_CEF_COALESCED.fetch_add(1, Ordering::Relaxed);
}

pub fn record_image_copy(elapsed: Duration) {
    IMAGE_COPY_NANOS.fetch_add(elapsed.as_nanos() as u64, Ordering::Relaxed);
    IMAGE_COPIES.fetch_add(1, Ordering::Relaxed);
}

pub fn record_viewport_render(elapsed: Duration) {
    VIEWPORT_RENDER_NANOS.fetch_add(elapsed.as_nanos() as u64, Ordering::Relaxed);
    VIEWPORT_RENDERS.fetch_add(1, Ordering::Relaxed);
}


#[cfg(not(test))]
pub fn record_root_metadata_update() {
    if enabled() {
        ROOT_METADATA_UPDATES.fetch_add(1, Ordering::Relaxed);
    }
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

struct ScrollMetrics {
    last_scroll_input: Instant,
    last_frame_instant: Option<Instant>,
    window_start: Instant,
    window_frames: u32,
    window_stutters: u32,
    window_max_interval_ms: f32,
}

static SCROLL_METRICS: OnceLock<Mutex<ScrollMetrics>> = OnceLock::new();
static SCROLL_LOG_SENDER: OnceLock<std::sync::mpsc::SyncSender<String>> = OnceLock::new();

fn emit_scroll_log(line: String) {
    let sender = SCROLL_LOG_SENDER.get_or_init(|| {
        let (sender, receiver) = std::sync::mpsc::sync_channel::<String>(32);
        let _ = thread::Builder::new()
            .name("scroll-log-writer".into())
            .spawn(move || {
                let mut file = std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open("/home/rover/Projects/kreon/lumen/scroll.log")
                    .or_else(|_| {
                        std::fs::OpenOptions::new()
                            .create(true)
                            .append(true)
                            .open("scroll.log")
                    })
                    .ok();
                while let Ok(line) = receiver.recv() {
                    eprint!("{line}");
                    if let Some(file) = &mut file {
                        let _ = file.write_all(line.as_bytes());
                    }
                }
            });
        sender
    });
    let _ = sender.try_send(line);
}

fn get_scroll_metrics() -> &'static Mutex<ScrollMetrics> {
    SCROLL_METRICS.get_or_init(|| {
        Mutex::new(ScrollMetrics {
            last_scroll_input: Instant::now() - Duration::from_secs(10),
            last_frame_instant: None,
            window_start: Instant::now(),
            window_frames: 0,
            window_stutters: 0,
            window_max_interval_ms: 0.0,
        })
    })
}

pub fn record_scroll_input() {
    let mut m = get_scroll_metrics().lock().unwrap();
    let now = Instant::now();
    if now.duration_since(m.last_scroll_input) > Duration::from_millis(1500) {
        SCROLL_CEF_PAINTS.store(0, Ordering::Relaxed);
        SCROLL_CEF_COALESCED.store(0, Ordering::Relaxed);
        m.window_start = now;
        m.window_frames = 0;
        m.window_stutters = 0;
        m.window_max_interval_ms = 0.0;
        m.last_frame_instant = None;
        emit_scroll_log("\n[SCROLL SESSION START]\n".to_string());
    }
    m.last_scroll_input = now;
}

#[allow(dead_code)]
pub fn record_scroll_frame(nominal_hz: u32) {
    let mut m = get_scroll_metrics().lock().unwrap();
    let now = Instant::now();
    let is_scrolling = now.duration_since(m.last_scroll_input) <= Duration::from_millis(1200);
    if !is_scrolling {
        if m.window_frames > 0 && m.last_frame_instant.is_some() {
            let elapsed = m.window_start.elapsed().as_secs_f32();
            let fps = if elapsed > 0.0 {
                m.window_frames as f32 / elapsed
            } else {
                0.0
            };
            let summary = format!(
                "[SCROLL SESSION SUMMARY] Frames: {} | Avg FPS: {:.1}\n",
                m.window_frames, fps
            );
            emit_scroll_log(summary);
            m.window_frames = 0;
            m.last_frame_instant = None;
        }
        return;
    }

    let hz = nominal_hz.clamp(30, 360);
    let nominal_interval_ms = 1000.0 / hz as f32;
    let stutter_threshold_ms = nominal_interval_ms * 1.5;

    m.window_frames += 1;

    // Aggregate spikes in memory; per-frame stderr and file writes can themselves
    // stall the UI thread during the exact workload this logger measures.
    if let Some(prev) = m.last_frame_instant {
        let interval_ms = now.duration_since(prev).as_secs_f32() * 1000.0;

        if interval_ms >= stutter_threshold_ms {
            m.window_stutters += 1;
            m.window_max_interval_ms = m.window_max_interval_ms.max(interval_ms);
        }
    }
    m.last_frame_instant = Some(now);

    // Send periodic summaries to the background writer, keeping I/O off the render thread.
    let window_elapsed = m.window_start.elapsed();
    if window_elapsed >= Duration::from_millis(500) {
        let fps = m.window_frames as f32 / window_elapsed.as_secs_f32();
        let loop_samples = LOOP_ITERS.swap(0, Ordering::Relaxed);
        let cef_paints = SCROLL_CEF_PAINTS.swap(0, Ordering::Relaxed);
        let coalesced = SCROLL_CEF_COALESCED.swap(0, Ordering::Relaxed);
        let stat_line = format!(
            "[SCROLL LOG] FPS: {:.1} | Nominal: {:.1}ms ({}Hz) | spikes: {} max: {:.1}ms | CEF paints/s: {:.1} | CEF coalesced/s: {:.1} | CEF paint: {:.2}ms | image copy: {:.2}ms | viewport build: {:.2}ms | loop update/wait: {:.2}/{:.2}ms\n",
            fps,
            nominal_interval_ms,
            hz,
            m.window_stutters,
            m.window_max_interval_ms,
            cef_paints as f64 / window_elapsed.as_secs_f64(),
            coalesced as f64 / window_elapsed.as_secs_f64(),
            average_ms(&CEF_PAINT_NANOS, &CEF_PAINTS),
            average_ms(&IMAGE_COPY_NANOS, &IMAGE_COPIES),
            average_ms(&VIEWPORT_RENDER_NANOS, &VIEWPORT_RENDERS),
            average_ms_with_count(&LOOP_UPDATE_NANOS, loop_samples),
            average_ms_with_count(&LOOP_WAIT_NANOS, loop_samples),
        );
        emit_scroll_log(stat_line);
        m.window_start = now;
        m.window_frames = 0;
        m.window_stutters = 0;
        m.window_max_interval_ms = 0.0;
    }
}

fn average_ms(total_nanos: &AtomicU64, samples: &AtomicU64) -> f64 {
    let count = samples.swap(0, Ordering::Relaxed);
    average_ms_with_count(total_nanos, count)
}

fn average_ms_with_count(total_nanos: &AtomicU64, count: u64) -> f64 {
    let total = total_nanos.swap(0, Ordering::Relaxed);
    if count == 0 {
        0.0
    } else {
        total as f64 / count as f64 / 1_000_000.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocked_request_records_counts_correctly() {
        set_plugin_enabled(true);
        unsafe {
            std::env::set_var("ANARC_STATS", "1");
        }
        record_blocked_request("tab-1");
        record_blocked_request("tab-1");
        record_blocked_request("tab-2");
        let counts = BLOCKED_BY_TAB.get().unwrap().lock().unwrap();
        assert!(counts.get("tab-1").copied().unwrap_or(0) >= 2);
        assert!(counts.get("tab-2").copied().unwrap_or(0) >= 1);
        unsafe {
            std::env::remove_var("ANARC_STATS");
        }
    }
}
