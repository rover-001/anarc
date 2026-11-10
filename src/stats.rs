//! Optional once-per-second browser and UI performance counters.

use std::{
    collections::HashMap,
    fs,
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
                    "Lumen stats: OSR paints/s={paints} root UI renders/s={renders} root metadata updates/s={metadata} avg frame-copy={avg_copy_ms:.3} ms blocked-by-tab=[{blocked}] RSS={}",
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
    if let Some(count) = counts.get_mut(tab_id) {
        *count += 1;
    } else {
        counts.insert(tab_id.to_owned(), 1);
    }
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
