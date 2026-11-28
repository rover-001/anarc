# Changelog

## v0.1.3

- Web content is capped to the display refresh rate by default; an opt-in
  "Uncapped frame rate" toggle in Settings > General removes the cap after a
  restart (`ANARC_UNCAPPED_FPS=1` remains available for benchmarks).
- Scrolling: discrete wheel notches no longer add app-side momentum, Wayland
  continuous deltas match Chromium's raw pixel values, fling coasting starts on
  the first frame after lift-off with no zero-step tail, and OSR paints are
  phase-locked to display frames via GPUI-driven BeginFrames.
- Web-page cursors map to the matching system-theme shapes for diagonal resize,
  panning, cell, and drag states.

## Unreleased — Telemetry schema version 1

- Added opt-in, default-off daily agent-control telemetry (`schema_version = 1`):
  one POST to the configured `INGEST_URL` (currently the production Convex
  deployment `https://amicable-sparrow-936.convex.site/ping`) carrying Cargo app version,
  distro and CPU architecture enum codes, a rotating random UUIDv4 install ID,
  and an optional `agent` object of bucketed sessions, harness codes,
  per-action call counts, snapshot token ratio, snapshot latency, element
  count, ref-resolve success, per-error counts, and installed plugin count.
  No URLs, page content, prompts, agent output, IPs, or exact timestamps are sent.
- Added local consent/install-ID state and a canonical daily rollup file used
  for both the consent preview and the outgoing request; three kill switches
  (`--no-telemetry`, `ANARC_TELEMETRY=0`, and the Settings > Privacy toggle) are
  checked at the single send site. Added a first-run consent screen, an
  auth-gated Convex ingestion/dashboard in `convex/`, and a public benchmark
  runner under `tools/agent-bench/`.
- **Process requirement:** any change to the telemetry schema — fields, enum
  codes, bucket bands, endpoint, or privacy behavior — must update
  [TELEMETRY.md](TELEMETRY.md) in the same commit.
