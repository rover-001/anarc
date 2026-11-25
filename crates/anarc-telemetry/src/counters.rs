//! In-memory counters for the agent-control plugin.
//!
//! Every input is a count, enum, character length or duration. No page string
//! (URL, title, DOM text, ref label, prompt or agent output) can be passed in.
//! Only bucketed aggregates are exposed via `stats()`.

use crate::buckets::{bucket_count, bucket_element_count, bucket_ref_resolve_success};
use crate::buckets::{bucket_snapshot_ms, bucket_token_ratio};
use crate::schema::{ActionCounts, ActionKind, AgentStats, ErrorCode, ErrorCounts, Harness};
use std::sync::Mutex;

#[derive(Default)]
struct RawCounts {
    sessions: u64,
    calls: [u64; 5],
    harness_mask: u8,
    enabled: bool,
    snapshot_samples: u64,
    ref_chars_total: u64,
    dom_chars_total: u64,
    snapshot_ms_total: u64,
    peak_elements: u64,
    ref_resolves: u64,
    ref_successes: u64,
    errors: [u64; 4],
    plugins_installed: u64,
}

fn action_index(kind: ActionKind) -> usize {
    u8::from(kind) as usize
}

fn error_index(code: ErrorCode) -> usize {
    u8::from(code) as usize
}

/// Maps a fixed agent-control tool name (from `AgentRequest::op`) to the
/// recorded action enum. The name is used only to pick the enum; it is never
/// stored, copied into a ping, or sent.
pub fn action_kind_for_tool(op: &str) -> ActionKind {
    match op {
        "read" | "see" | "find" | "shot" | "eval" | "set_value" | "wait_for" => ActionKind::Read,
        _ => ActionKind::Other,
    }
}

/// Maps a caller-supplied harness identifier to the recorded harness enum.
/// Only the enum is retained; the identifier itself never enters a ping.
pub fn harness_from_name(name: &str) -> Harness {
    match name.trim().to_ascii_lowercase().as_str() {
        "claude" | "claude-code" | "claudecode" => Harness::Claude,
        "codex" | "openai-codex" => Harness::Codex,
        "antigravity" => Harness::Antigravity,
        _ => Harness::Other,
    }
}

#[derive(Default)]
pub struct Counters {
    raw: Mutex<RawCounts>,
}

impl Counters {
    pub fn new() -> Self {
        Self {
            raw: Mutex::new(RawCounts {
                enabled: true,
                ..RawCounts::default()
            }),
        }
    }

    pub fn set_enabled(&self, enabled: bool) {
        if let Ok(mut raw) = self.raw.lock() {
            raw.enabled = enabled;
        }
    }

    pub fn note_session(&self) {
        if let Ok(mut raw) = self.raw.lock() {
            raw.sessions = raw.sessions.saturating_add(1);
        }
    }

    pub fn note_call(&self, kind: ActionKind) {
        if let Ok(mut raw) = self.raw.lock() {
            let index = action_index(kind);
            if let Some(slot) = raw.calls.get_mut(index) {
                *slot = slot.saturating_add(1);
            }
        }
    }

    pub fn note_harness(&self, harness: Harness) {
        if let Ok(mut raw) = self.raw.lock() {
            raw.harness_mask |= 1 << u8::from(harness);
        }
    }

    /// Only lengths and counts are accepted here; no DOM text is read.
    pub fn note_snapshot(&self, ref_chars: u64, dom_chars: u64, elapsed_ms: u64, elements: u64) {
        if let Ok(mut raw) = self.raw.lock() {
            raw.snapshot_samples = raw.snapshot_samples.saturating_add(1);
            raw.ref_chars_total = raw.ref_chars_total.saturating_add(ref_chars);
            raw.dom_chars_total = raw.dom_chars_total.saturating_add(dom_chars);
            raw.snapshot_ms_total = raw.snapshot_ms_total.saturating_add(elapsed_ms);
            raw.peak_elements = raw.peak_elements.max(elements);
        }
    }

    pub fn note_ref_resolve(&self, success: bool) {
        if let Ok(mut raw) = self.raw.lock() {
            raw.ref_resolves = raw.ref_resolves.saturating_add(1);
            if success {
                raw.ref_successes = raw.ref_successes.saturating_add(1);
            }
        }
    }

    pub fn note_error(&self, code: ErrorCode) {
        if let Ok(mut raw) = self.raw.lock() {
            let index = error_index(code);
            if let Some(slot) = raw.errors.get_mut(index) {
                *slot = slot.saturating_add(1);
            }
        }
    }

    pub fn set_plugins_installed(&self, count: u64) {
        if let Ok(mut raw) = self.raw.lock() {
            raw.plugins_installed = count;
        }
    }

    pub fn stats(&self) -> AgentStats {
        let raw = self
            .raw
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let avg_ms = raw
            .snapshot_ms_total
            .checked_div(raw.snapshot_samples)
            .unwrap_or(0);
        let success_percent = raw
            .ref_successes
            .saturating_mul(100)
            .checked_div(raw.ref_resolves)
            .unwrap_or(0);
        let harness = (0u8..=3)
            .filter(|code| raw.harness_mask & (1 << code) != 0)
            .filter_map(|code| Harness::try_from(code).ok())
            .collect();
        AgentStats {
            enabled: raw.enabled,
            harness,
            sessions: bucket_count(raw.sessions),
            calls_by_action: ActionCounts {
                click: bucket_count(raw.calls[action_index(ActionKind::Click)]),
                ty: bucket_count(raw.calls[action_index(ActionKind::Type)]),
                navigate: bucket_count(raw.calls[action_index(ActionKind::Navigate)]),
                read: bucket_count(raw.calls[action_index(ActionKind::Read)]),
                other: bucket_count(raw.calls[action_index(ActionKind::Other)]),
            },
            snapshot_token_ratio: bucket_token_ratio(raw.ref_chars_total, raw.dom_chars_total),
            snapshot_ms: bucket_snapshot_ms(avg_ms),
            element_count: bucket_element_count(raw.peak_elements),
            ref_resolve_success: bucket_ref_resolve_success(success_percent),
            errors_by_code: ErrorCounts {
                stale_ref: bucket_count(raw.errors[error_index(ErrorCode::StaleRef)]),
                not_found: bucket_count(raw.errors[error_index(ErrorCode::NotFound)]),
                timeout: bucket_count(raw.errors[error_index(ErrorCode::Timeout)]),
                other: bucket_count(raw.errors[error_index(ErrorCode::Other)]),
            },
            plugins_installed: bucket_count(raw.plugins_installed),
        }
    }

    pub fn reset(&self) {
        if let Ok(mut raw) = self.raw.lock() {
            *raw = RawCounts {
                enabled: true,
                ..RawCounts::default()
            };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_names_map_to_enum_only() {
        assert_eq!(action_kind_for_tool("read"), ActionKind::Read);
        assert_eq!(action_kind_for_tool("see"), ActionKind::Read);
        assert_eq!(action_kind_for_tool("find"), ActionKind::Read);
        assert_eq!(action_kind_for_tool("shot"), ActionKind::Read);
        assert_eq!(action_kind_for_tool("eval"), ActionKind::Read);
        assert_eq!(action_kind_for_tool("set_value"), ActionKind::Read);
        assert_eq!(action_kind_for_tool("wait_for"), ActionKind::Read);
        assert_eq!(action_kind_for_tool("do"), ActionKind::Other);
        assert_eq!(action_kind_for_tool("tabs"), ActionKind::Other);
        assert_eq!(
            action_kind_for_tool("https://example.com"),
            ActionKind::Other
        );
    }

    #[test]
    fn harness_names_map_to_enum_only() {
        assert_eq!(harness_from_name("claude"), Harness::Claude);
        assert_eq!(harness_from_name(" Claude-Code "), Harness::Claude);
        assert_eq!(harness_from_name("CODEX"), Harness::Codex);
        assert_eq!(harness_from_name("antigravity"), Harness::Antigravity);
        assert_eq!(harness_from_name("unknown"), Harness::Other);
        assert_eq!(harness_from_name("https://example.com"), Harness::Other);
    }

    #[test]
    fn counters_only_expose_buckets() {
        let counters = Counters::new();
        counters.note_session();
        counters.note_call(ActionKind::Click);
        counters.note_harness(Harness::Codex);
        // Lengths only: no DOM text enters the telemetry path.
        counters.note_snapshot(1_000, 10_000, 12, 120);
        counters.note_ref_resolve(true);
        counters.note_ref_resolve(false);
        counters.note_error(ErrorCode::StaleRef);
        counters.set_plugins_installed(2);

        let stats = counters.stats();
        assert!(stats.enabled);
        assert_eq!(stats.harness, vec![Harness::Codex]);
        assert_eq!(stats.sessions.code(), 1);
        assert_eq!(stats.calls_by_action.click.code(), 1);
        assert_eq!(stats.calls_by_action.read.code(), 0);
        assert_eq!(stats.snapshot_token_ratio.code(), 1);
        assert_eq!(stats.snapshot_ms.code(), 1);
        assert_eq!(stats.element_count.code(), 1);
        assert_eq!(stats.ref_resolve_success.code(), 5);
        assert_eq!(stats.errors_by_code.stale_ref.code(), 1);
        assert_eq!(stats.errors_by_code.other.code(), 0);
        assert_eq!(stats.plugins_installed.code(), 1);

        counters.reset();
        assert_eq!(counters.stats().sessions.code(), 0);
    }
}
