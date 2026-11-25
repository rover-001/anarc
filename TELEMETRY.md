# Anarc telemetry

Opt-in, privacy-safe analytics for the agent-control layer. **Off by default:**
nothing is sent until consent is `granted`. The wire schema in
[`crates/anarc-telemetry/src/schema.rs`](crates/anarc-telemetry/src/schema.rs)
and this file are the source of truth; they must be updated together.

## Wire format

One daily rollup ping, `POST https://amicable-sparrow-936.convex.site/ping` (`INGEST_URL` in
[`sender.rs`](crates/anarc-telemetry/src/sender.rs)). At most one ping per UTC
day, serialized by `ping_json` in the schema file; the consent preview and the
request body are byte-for-byte identical. The request is fire-and-forget on a
background thread: 5-second timeout, no retries, result discarded. Transport
failures never block the browser UI or agent calls. `schema_version` is `1`.

Top-level payload (`Ping`, `deny_unknown_fields`):

| Field | Wire type | Value |
| --- | --- | --- |
| `schema_version` | `u8` | `1` (`SCHEMA_VERSION`) |
| `app_version` | string | Anarc Cargo package version; the only free-form string |
| `distro` | enum code | `arch=0`, `ubuntu=1`, `fedora=2`, `debian=3`, `nixos=4`, `other=5`; read from `ID=` in `/etc/os-release`, `other` on missing/unknown ID |
| `arch` | enum code | `x86_64=0`, `aarch64=1`; an unsupported target produces no ping |
| `install_id` | UUID v4 | random, regenerated every 30 UTC days, never derived from old IDs, clock, or machine data |
| `agent` | object or `null` | bucketed agent-control stats; `null` only when no install ID or supported arch is available |

All enums are numeric codes on the wire. The only JSON strings are
`app_version` and `install_id`; every other value is a number, boolean, object,
array, or `null`. `Bucket` deserialization rejects codes above `9`.

### `agent` object (`AgentStats`, `deny_unknown_fields`)

| Field | Wire type | Meaning |
| --- | --- | --- |
| `enabled` | bool | agent-control enabled flag |
| `harness` | array of enum codes | `claude=0`, `codex=1`, `antigravity=2`, `other=3` |
| `sessions` | bucket (count bands) | session count |
| `calls_by_action` | object | `click`, `ty` (type), `navigate`, `read`, `other` — each a bucket of count bands; action enum codes are `click=0`, `type=1`, `navigate=2`, `read=3`, `other=4` |
| `snapshot_token_ratio` | bucket (ratio bands) | ref-snapshot characters / raw-DOM characters, as an integer percentage |
| `snapshot_ms` | bucket (latency bands) | average snapshot wall time in milliseconds |
| `element_count` | bucket (element bands) | peak element count |
| `ref_resolve_success` | bucket (10-point bands) | percentage of successful ref resolutions |
| `errors_by_code` | object | `stale_ref`, `not_found`, `timeout`, `other` — each a bucket of count bands; error enum codes are `stale_ref=0`, `not_found=1`, `timeout=2`, `other=3` |
| `plugins_installed` | bucket (count bands) | installed plugin count |

`calls_by_action` (`ActionCounts`) and `errors_by_code` (`ErrorCounts`) also use
`deny_unknown_fields`. The wire name for type calls is `ty`.

### Bucket bands

Band codes and boundaries are defined in
[`buckets.rs`](crates/anarc-telemetry/src/buckets.rs); only the code, never the
underlying measurement, leaves the machine.

| Bucket | Code 0 | Code 1 | Code 2 | Code 3 | Code 4 |
| --- | --- | --- | --- | --- | --- |
| Count (sessions, per-action calls, per-error counts, plugins) | 0 | 1–5 | 6–20 | 21–99 | 100+ |
| `snapshot_token_ratio` | 0–9% | 10–24% | 25–49% | 50%+ (ratios above 100% clamped here) | — |
| `snapshot_ms` | 0–9 ms | 10–49 ms | 50–199 ms | 200+ ms | — |
| `element_count` | 0–49 | 50–199 | 200–999 | 1000+ | — |

`ref_resolve_success` uses ten 10-point bands: percentage `0–9 → 0`, `10–19 → 1`, …,
`90–99 → 9`. Inputs above 100 are clamped, so `100+ → 9`; zero resolution
attempts yield `0`. A zero DOM-character denominator yields ratio band `0`.
The session band deliberately separates `21–99` (code 3) from `100+` (code 4);
only the 10-point success scale reaches code 9.

## What is never sent

URLs, domains, page titles, DOM text, ref labels, prompts, agent output, IP
addresses, fingerprinting data, and exact timestamps. Instrumentation accepts
only counts, character lengths, enums, and durations. Consent and the install
ID record store a whole UTC day number locally, never a timestamp on the wire.
The ingestion endpoint reads only `content-length` (to reject oversized bodies)
and never reads or stores IPs, user agents, or any other request header.

## Local files and preview

- `$ANARC_CONFIG_DIR/telemetry.json` (default `~/.config/anarc/telemetry.json`):
  consent (`unset`/`granted`/`denied`, default `unset`) plus the random install
  ID and its generation day, saved via a temporary file and rename. Never sent.
  Missing or corrupt files fall back to "no consent, new random ID".
- `$ANARC_DATA_DIR/telemetry/rollup-<day>.json` (default
  `~/.local/share/anarc/telemetry/rollup-<day>.json`): canonical JSON of the
  next ping, written once per UTC day before the send.
- `preview_json()` in [`src/agent/telemetry.rs`](src/agent/telemetry.rs) renders
  "Show what would be sent" from the same `ping_json` bytes used for the POST;
  Settings > Privacy and the palette command display it verbatim.

## How to disable

Any one of these wins. Consent defaults to `unset`; `flush_daily` returns before
writing a rollup or scheduling a send unless consent is `granted`, and the
single send site rechecks all three switches in `SendGate::enabled`.

- **Settings > Privacy toggle** — the first-run consent screen and the
  Settings > Privacy section write consent through `set_consent`; "Deny"
  persists `denied`, so no ping is ever scheduled.
- **`--no-telemetry`** on the Anarc command line — parsed once at startup in
  [`src/main.rs`](src/main.rs).
- **`ANARC_TELEMETRY=0`** in the environment — exact match only; `1`, `false`,
  or an unset variable do not disable telemetry.

## Rollout status (verified against files)

All seven phases are implemented:

- **Phases 1–2:** `crates/anarc-telemetry` (schema, buckets, consent/config,
  counters, rollup store, gated sender) plus count-only hooks in the agent
  request path and a best-effort daily flush on plugin mount / session end.
- **Phases 3 and 5:** [`convex/`](convex/README.md) contains the `POST /ping`
  HTTP action, portable validator, 4 KB body cap, 1/day per-install and global
  rate limits, rollup-only tables, and an auth-gated dashboard query. The
  dashboard requires `TELEMETRY_ADMIN_TOKEN`; Convex Auth helpers are not wired
  in this repo.
- **Phase 4:** first-run consent screen and Settings > Privacy (toggle,
  verbatim preview, reset install ID), plus two command-palette entries.
- **Phase 6:** this file, the README link, and `CHANGELOG.md`.
- **Phase 7:** [`telemetry_bench.py`](tools/agent-bench/telemetry_bench.py) and
  [`TELEMETRY_BENCH.md`](tools/agent-bench/TELEMETRY_BENCH.md). Its optional
  `--confirm-telemetry` mode reads local `rollup-*.json` files and compares only
  numeric bucket codes; field telemetry never supplies a benchmark number and
  the runner sends nothing.

Deployment note: the client sends to the `anarc` Convex project's **production**
deployment `amicable-sparrow-936`, at
`https://amicable-sparrow-936.convex.site/ping`. The separate dev deployment
`bold-terrier-317` is for testing only. One admin token is set as a Convex
deployment env var via stdin; it is never committed. See
[`convex/README.md`](convex/README.md) for setup and secret handling.
