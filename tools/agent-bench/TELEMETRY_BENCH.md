# Public benchmark protocol — raw DOM vs ref snapshots

Companion to `bench.py` (the 25-fixture suite) and `TELEMETRY.md` (the privacy
schema). Runner: `telemetry_bench.py`. Results template: `results_template.json`.

**Scope.** One fixed task suite (`bench.py::TASKS`, fixtures in `fixtures/`)
run under three harnesses — Claude, Codex, Antigravity — in two conditions:

- `raw_dom`: a full DOM snapshot is fed to the agent on every step.
- `ref_snapshot`: the Anarc five-tool pull model (`see`, `do`, `read`, `find`,
  `shot`) with typed refs and diff replies.

Published metrics per condition: **tool-result tokens**, **success rate**,
**wall latency**, **tool calls**, **retries**. Comparisons are always made
*within* a harness (raw DOM vs ref snapshots); cross-harness token counts are
not normalized and must not be compared directly.

## Methodology

- **Tokens** = sum of `tool_result_tokens` per case. These are the tokens the
  agent actually ingests. For real runs, take `est_tokens` from the agent-control
  log (`$ANARC_DATA_DIR/agent/log.jsonl`, see `control_smoke.py`) or the
  harness's own usage report. Estimate formula for synthetic data follows
  `bench.py::estimate_tokens` conventions: raw DOM ≈ 24 tokens/DOM node + 200
  per step; ref snapshot ≈ 10 tokens/element + 45 for the first `see`, ~22 per
  diff `do` reply. Synthetic numbers are labeled as such and are never published.
- **Success rate** = successful cases / attempted cases × 100. A case succeeds
  only when the fixture's declared task outcome is verified by the harness.
- **Wall latency** = end-to-end ms per case (median and p95 across cases in a
  cell). Includes fixture load, tool round trips and verification.
- **Tool calls / retries** = counted from the harness trace. A retry is a repeated
  tool call after a failed or stale-ref result; it re-ingests a tool result and
  is therefore included in tokens.
- **Conditions are paired**: every `(harness, task_id)` has exactly one `raw_dom`
  and one `ref_snapshot` case. Unpaired cases are rejected.
- **Determinism**: `--self-test` uses a fixed seed (default `20261125`).

## How to run

Self-test — offline, standard library only, no browser, no network, no API
keys, no cargo:

```sh
python3 tools/agent-bench/telemetry_bench.py --self-test
```

Real run — opt-in, uses the existing bench harness/CLI. A wrapper script drives
one case per invocation; the runner passes `BENCH_HARNESS`, `BENCH_CONDITION`,
`BENCH_TASK_ID`, `BENCH_FIXTURE` and `BENCH_OUT` in the environment, and the
wrapper must write a JSON case file to `$BENCH_OUT`:

```json
{"success": true, "tool_result_tokens": [220, 41],
 "wall_latency_ms": 830.5, "tool_calls": 2, "retries": 0}
```

```sh
python3 tools/agent-bench/telemetry_bench.py \
  --harness-cmd 'python3 tools/agent-bench/run_case.py' \
  --json-out tools/agent-bench/results.json
```

`run_case.py` is a local wrapper the benchmark publisher supplies. In this
repository it should use `anarc-ctl` with the fixture-serving and environment
patterns from `control_smoke.py` (`ANARC_CONFIG_DIR`, `ANARC_DATA_DIR`,
`ANARC_SOCKET_PATH`; commands `see`, `do`, `read`, `stats`, MCP `shot`). In
`raw_dom` the wrapper collects the full serialized DOM as tool-result text; in
`ref_snapshot` it collects the typed-ref `see`/`do` replies. Start from
`results_template.json`; schema version is 1.

Render or re-render a collected run without executing anything:

```sh
python3 tools/agent-bench/telemetry_bench.py --results tools/agent-bench/results.json
```

## Field telemetry is confirmation only

Telemetry is **never required** to run the benchmark and **never supplies a
published number**. `--confirm-telemetry DIR` is optional and read-only; point `DIR` at the local
daily rollup directory (typically `$ANARC_DATA_DIR/telemetry`, default
`~/.local/share/anarc/telemetry`) containing `rollup-*.json`. It reads those
files *after* the benchmark results are computed. It
compares benchmark bucket codes with the bucket codes observed in the field,
using the exact boundaries from `crates/anarc-telemetry/src/buckets.rs`:

- `snapshot_token_ratio` → ref/raw tool-result-token percentage band.
- `snapshot_ms` → ref median wall-time band.
- `harness` → which harness enum codes (Claude 0, Codex 1, Antigravity 2)
  appear in field data.

The banding is directional, not definitional: field `snapshot_token_ratio` is
computed from character lengths, while the benchmark uses tool-result tokens.
A field band outside the benchmark band is an *informational prompt to
investigate*, not a benchmark failure; it never edits, hides or replaces a
result. Missing telemetry reports `NOT RUN` and the benchmark still succeeds.

**Privacy.** No page content is sent to telemetry. No DOM text, ref label,
prompt, URL, title or agent output is read from telemetry by this runner either:
only numeric bucket codes and harness enum codes are parsed, and the only
strings in the schema are `app_version` and a random `install_id`, which this
runner ignores. See `TELEMETRY.md` for the wire schema and opt-outs
(`--no-telemetry`, `ANARC_TELEMETRY=0`, Settings > Privacy).

## Results — empty publication table

Fill only from a real run. No values are pre-filled; synthetic self-test numbers
are not publication results.

| Harness | Condition | Tasks | Success rate | Tool-result tokens | Tokens/task | Wall ms (median) | Wall ms (p95) | Tool calls | Retries |
|:--|:--|--:|--:|--:|--:|--:|--:|--:|--:|
| Claude | raw_dom |  |  |  |  |  |  |  |  |
| Claude | ref_snapshot |  |  |  |  |  |  |  |  |
| Codex | raw_dom |  |  |  |  |  |  |  |  |
| Codex | ref_snapshot |  |  |  |  |  |  |  |  |
| Antigravity | raw_dom |  |  |  |  |  |  |  |  |
| Antigravity | ref_snapshot |  |  |  |  |  |  |  |  |

| Harness | Raw tokens | Ref tokens | Raw/ref ratio | Success Δ (pp) | Median ms Δ | Tool calls Δ | Retries Δ | Field confirmation |
|:--|--:|--:|--:|--:|--:|--:|--:|:--|
| Claude |  |  |  |  |  |  |  |  |
| Codex |  |  |  |  |  |  |  |  |
| Antigravity |  |  |  |  |  |  |  |  |
