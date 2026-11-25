#!/usr/bin/env python3
"""Public benchmark protocol/runner: raw DOM vs ref snapshots across harnesses.

Self-contained, Python standard library only.  Three entry points:

  * ``--self-test``  offline deterministic synthetic run (no browser, no network,
                     no API keys, no cargo).  This is the mode CI can run.
  * ``--harness-cmd`` execute a per-case command template that drives the
                     existing ``anarc-ctl``/MCP harness and writes a JSON result
                     file (see TELEMETRY_BENCH.md).
  * ``--results``    render tables from a previously collected results JSON.

``--confirm-telemetry`` is ALWAYS optional and read-only.  Field telemetry is
used only to confirm that benchmark bucket codes still occur in real-world
local rollups.  It never supplies a benchmark number and never changes a
published result.

PRIVACY: this runner reads and writes only local files.  No page content, DOM
text, ref label, prompt, URL or agent output is read from telemetry or sent
anywhere.  Telemetry contains only lengths, counts, enum codes and durations;
this runner touches only the numeric bucket codes ``snapshot_token_ratio`` and
``snapshot_ms`` plus the harness code list (schema_version 1).
"""

import argparse
import json
import math
import os
from pathlib import Path
import random
import shlex
import statistics
import subprocess
import sys
import tempfile

HERE = Path(__file__).resolve().parent
FIXTURES_DIR = HERE / "fixtures"
RESULTS_TEMPLATE = HERE / "results_template.json"

HARNESSES = ("claude", "codex", "antigravity")
# Mirrors crates/anarc-telemetry/src/schema.rs :: Harness.
HARNESS_CODES = {"claude": 0, "codex": 1, "antigravity": 2}
CONDITIONS = ("raw_dom", "ref_snapshot")
SCHEMA_VERSION = 1
DEFAULT_SEED = 20261125

# Task specs come from the existing suite; the fallback keeps --self-test
# working even if this file is copied out of the repository.
try:
    from bench import TASKS as BENCH_TASKS  # type: ignore
except Exception:  # pragma: no cover - only used outside the repo
    BENCH_TASKS = [
        {"id": "01_login", "name": "User Sign In", "steps": 3, "elements": 8, "dom_nodes": 45},
        {"id": "07_table_data", "name": "Financial Data Table", "steps": 3, "elements": 30, "dom_nodes": 280},
    ]


# ---------------------------------------------------------------------------
# Bucket helpers: exact boundaries from crates/anarc-telemetry/src/buckets.rs.
# Inputs are counts/lengths/durations only -- never page content.
# ---------------------------------------------------------------------------

def bucket_token_ratio(ref_chars: int, dom_chars: int) -> int:
    """ref/dom as a percentage: <10 -> 0, 10-24 -> 1, 25-49 -> 2, 50+ -> 3."""
    if dom_chars <= 0:
        return 0
    percent = (int(ref_chars) * 100) // int(dom_chars)
    if percent <= 9:
        return 0
    if percent <= 24:
        return 1
    if percent <= 49:
        return 2
    return 3


def bucket_snapshot_ms(ms: float) -> int:
    """Wall time in milliseconds: <10 -> 0, 10-49 -> 1, 50-199 -> 2, 200+ -> 3."""
    value = int(ms)
    if value <= 9:
        return 0
    if value <= 49:
        return 1
    if value <= 199:
        return 2
    return 3


# ---------------------------------------------------------------------------
# Synthetic offline data
# ---------------------------------------------------------------------------

def synthetic_runs(seed: int, tasks: list, harnesses: tuple, conditions: tuple) -> list:
    """Deterministic synthetic runs; formulas mirror tools/agent-bench/bench.py.

    raw_dom: one full DOM snapshot per step (~24 tokens/DOM node + 200).
    ref_snapshot: one compact ``see`` (~10 tokens/element + 45) followed by
    small ``do`` diff replies (~22 tokens), matching bench.py's baselines.
    """
    rng = random.Random(seed)
    factor = {"claude": 1.00, "codex": 0.97, "antigravity": 1.04}
    runs = []
    for harness in harnesses:
        for condition in conditions:
            for task in tasks:
                steps = int(task["steps"])
                if condition == "raw_dom":
                    step_tokens = [task["dom_nodes"] * 24 + 200 for _ in range(steps)]
                    latency = 120.0 + task["dom_nodes"] * 0.55
                    calls = steps
                    retry_chance = 0.08
                else:
                    step_tokens = [task["elements"] * 10 + 45] + [22] * (steps - 1)
                    latency = 35.0 + task["elements"] * 0.45
                    calls = 2
                    retry_chance = 0.04

                scaled = [max(1, round(tok * factor[harness] * rng.uniform(0.97, 1.03)))
                          for tok in step_tokens]
                retries = 1 if rng.random() < retry_chance else 0
                if retries:
                    # A retry re-ingests one tool result.
                    scaled.append(scaled[0])
                    calls += 1
                    latency += 40.0
                success = rng.random() > (0.05 if condition == "raw_dom" else 0.02)
                runs.append({
                    "harness": harness,
                    "condition": condition,
                    "task_id": task["id"],
                    "success": bool(success),
                    "tool_result_tokens": scaled,
                    "wall_latency_ms": round(latency * factor[harness] * rng.uniform(0.95, 1.05), 2),
                    "tool_calls": calls,
                    "retries": retries,
                })
    return runs


def real_runs_from_command(template: str, tasks: list, harnesses: tuple,
                           conditions: tuple, fixtures: Path) -> list:
    """Drive an external harness; each case must write $BENCH_OUT as JSON.

    The command is split with shlex (no shell); case data is passed via env so
    wrappers can call ``anarc-ctl``/MCP using the conventions in
    control_smoke.py.  This mode is opt-in and is not used by --self-test.
    """
    runs = []
    with tempfile.TemporaryDirectory(prefix="telemetry-bench-") as tmp:
        out = Path(tmp) / "case.json"
        for harness in harnesses:
            for condition in conditions:
                for task in tasks:
                    fixture = fixtures / (task["id"] + ".html")
                    out.unlink(missing_ok=True)
                    env = dict(os.environ)
                    env.update({
                        "BENCH_HARNESS": harness,
                        "BENCH_CONDITION": condition,
                        "BENCH_TASK_ID": task["id"],
                        "BENCH_FIXTURE": str(fixture),
                        "BENCH_OUT": str(out),
                    })
                    argv = shlex.split(template)
                    if not argv:
                        raise ValueError("--harness-cmd is empty")
                    subprocess.run(argv, env=env, check=True)
                    case = json.loads(out.read_text(encoding="utf-8"))
                    runs.append(validate_case(case, harness, condition, task["id"]))
    return runs


def validate_case(case: dict, harness: str, condition: str, task_id: str) -> dict:
    """Normalize one real-run case.  Only numeric/count fields are accepted."""
    required = ("success", "tool_result_tokens", "wall_latency_ms", "tool_calls", "retries")
    missing = [key for key in required if key not in case]
    if missing:
        raise ValueError(f"case {harness}/{condition}/{task_id} missing {missing}")
    tokens = [int(value) for value in case["tool_result_tokens"]]
    if not tokens or any(value < 0 for value in tokens):
        raise ValueError(f"case {harness}/{condition}/{task_id} has invalid tokens")
    return {
        "harness": str(harness),
        "condition": str(condition),
        "task_id": str(task_id),
        "success": bool(case["success"]),
        "tool_result_tokens": tokens,
        "wall_latency_ms": float(case["wall_latency_ms"]),
        "tool_calls": int(case["tool_calls"]),
        "retries": int(case["retries"]),
    }


# ---------------------------------------------------------------------------
# Aggregation and reporting
# ---------------------------------------------------------------------------

def aggregate(runs: list, harnesses: tuple = HARNESSES,
              conditions: tuple = CONDITIONS) -> dict:
    cells = {}
    for harness in harnesses:
        for condition in conditions:
            selected = [r for r in runs if r["harness"] == harness and r["condition"] == condition]
            if not selected:
                continue
            tokens = [tok for r in selected for tok in r["tool_result_tokens"]]
            latencies = sorted(r["wall_latency_ms"] for r in selected)
            p95_index = min(len(latencies) - 1, int(math.ceil(len(latencies) * 0.95)) - 1)
            cells[(harness, condition)] = {
                "tasks": len(selected),
                "successes": sum(1 for r in selected if r["success"]),
                "success_rate": 100.0 * sum(1 for r in selected if r["success"]) / len(selected),
                "tokens_total": sum(tokens),
                "tokens_per_task": sum(tokens) / len(selected),
                "latency_median_ms": statistics.median(latencies),
                "latency_p95_ms": latencies[max(0, p95_index)],
                "tool_calls": sum(r["tool_calls"] for r in selected),
                "retries": sum(r["retries"] for r in selected),
            }
    return cells


def render_tables(cells: dict, harnesses: tuple, conditions: tuple) -> str:
    lines = ["## Per-condition results", "",
             "| Harness | Condition | Tasks | Success rate | Tool-result tokens | Tokens/task | Wall ms (median) | Wall ms (p95) | Tool calls | Retries |",
             "|:--|:--|--:|--:|--:|--:|--:|--:|--:|--:|"]
    for harness in harnesses:
        for condition in conditions:
            cell = cells.get((harness, condition))
            if not cell:
                continue
            lines.append(
                f"| {harness} | {condition} | {cell['tasks']} | {cell['success_rate']:.1f}% | "
                f"{cell['tokens_total']:,} | {cell['tokens_per_task']:.1f} | "
                f"{cell['latency_median_ms']:.1f} | {cell['latency_p95_ms']:.1f} | "
                f"{cell['tool_calls']} | {cell['retries']} |"
            )

    lines += ["", "## raw DOM vs ref snapshots (per harness)", "",
              "| Harness | Raw tokens | Ref tokens | Raw/ref ratio | Success Δ (pp) | Median ms Δ | Tool calls Δ | Retries Δ |",
              "|:--|--:|--:|--:|--:|--:|--:|--:|"]
    for harness in harnesses:
        raw = cells.get((harness, "raw_dom"))
        ref = cells.get((harness, "ref_snapshot"))
        if not raw or not ref:
            continue
        ratio = raw["tokens_total"] / ref["tokens_total"] if ref["tokens_total"] else 0.0
        lines.append(
            f"| {harness} | {raw['tokens_total']:,} | {ref['tokens_total']:,} | {ratio:.2f}x | "
            f"{ref['success_rate'] - raw['success_rate']:+.1f} | "
            f"{ref['latency_median_ms'] - raw['latency_median_ms']:+.1f} | "
            f"{ref['tool_calls'] - raw['tool_calls']:+d} | {ref['retries'] - raw['retries']:+d} |"
        )
    return "\n".join(lines) + "\n"


def confirm_telemetry(cells: dict, directory: Path, harnesses: tuple) -> str:
    """Optional, read-only confirmation against local daily rollups.

    Only numeric bucket codes and harness enum codes are read.  Nothing from a
    page is read, inferred or transmitted.  A mismatch is informational: it
    never edits, hides or replaces a benchmark result.
    """
    directory = Path(directory)
    files = sorted(directory.glob("rollup-*.json")) if directory.is_dir() else []
    if not files:
        return ("Telemetry confirmation: NOT RUN (no local rollup-*.json in "
                f"{directory}). Benchmark results are unchanged.\n")

    harness_codes = set()
    ratio_codes = set()
    ms_codes = set()
    malformed = 0
    for path in files:
        try:
            ping = json.loads(path.read_text(encoding="utf-8"))
            agent = ping["agent"]
            # Numeric enums/buckets only; app_version/install_id are ignored.
            harness_codes.update(int(code) for code in agent.get("harness", []))
            ratio_codes.add(int(agent["snapshot_token_ratio"]))
            ms_codes.add(int(agent["snapshot_ms"]))
        except Exception:
            malformed += 1

    bench_ratio = None
    bench_ms = None
    for harness in harnesses:
        raw = cells.get((harness, "raw_dom"))
        ref = cells.get((harness, "ref_snapshot"))
        if raw and ref:
            bench_ratio = bucket_token_ratio(ref["tokens_total"], raw["tokens_total"])
            bench_ms = bucket_snapshot_ms(ref["latency_median_ms"])
            break

    lines = ["Telemetry confirmation (informational only; local rollups, no page content):", "",
             "| Signal | Benchmark bucket | Field buckets | Status |",
             "|:--|--:|:--|:--|"]
    if bench_ratio is None:
        lines.append("| (no paired raw/ref cells) | - | - | NOT RUN |")
    else:
        status = "consistent" if bench_ratio in ratio_codes else "outside observed bands"
        lines.append(f"| snapshot_token_ratio (ref/raw tool-result tokens) | {bench_ratio} | "
                     f"{sorted(ratio_codes)} | {status} |")
        status = "consistent" if bench_ms in ms_codes else "outside observed bands"
        lines.append(f"| snapshot_ms (ref median wall time) | {bench_ms} | "
                     f"{sorted(ms_codes)} | {status} |")
    expected = {HARNESS_CODES[h] for h in harnesses if h in HARNESS_CODES}
    coverage = "all benchmarked harnesses observed" if expected <= harness_codes else \
               f"field harness codes {sorted(harness_codes)} (partial coverage)"
    lines.append(f"| harness coverage | {sorted(expected)} | {sorted(harness_codes)} | {coverage} |")
    lines.append("")
    lines.append(f"Rollups read: {len(files) - malformed}; unreadable: {malformed}. "
                 "Field telemetry is confirmation only: it never supplies a benchmark "
                 "number, and no page content is sent to telemetry.")
    return "\n".join(lines) + "\n"


def results_document(runs: list, mode: str, seed) -> dict:
    return {
        "schema_version": SCHEMA_VERSION,
        "mode": mode,
        "seed": seed,
        "harnesses": list(HARNESSES),
        "conditions": list(CONDITIONS),
        "tasks": len({r["task_id"] for r in runs}),
        "runs": runs,
    }


def load_results(path: Path) -> dict:
    doc = json.loads(Path(path).read_text(encoding="utf-8"))
    if int(doc.get("schema_version", -1)) != SCHEMA_VERSION:
        raise ValueError("unsupported schema_version")
    if not isinstance(doc.get("runs"), list):
        raise ValueError("results JSON has no runs list")
    return doc


# ---------------------------------------------------------------------------
# Self-test
# ---------------------------------------------------------------------------

def self_test() -> int:
    tasks = list(BENCH_TASKS)
    runs = synthetic_runs(DEFAULT_SEED, tasks, HARNESSES, CONDITIONS)
    cells = aggregate(runs)

    checks = []

    def check(name: str, condition: bool) -> None:
        checks.append((name, bool(condition)))

    check("all 3 harnesses x 2 conditions x tasks generated",
          len(runs) == len(HARNESSES) * len(CONDITIONS) * len(tasks))
    check("every cell aggregated", len(cells) == len(HARNESSES) * len(CONDITIONS))
    check("metrics positive and bounded",
          all(0 < c["tokens_total"] and 0 <= c["success_rate"] <= 100
              and c["latency_median_ms"] > 0 for c in cells.values()))
    check("ref snapshots cheaper than raw DOM per harness",
          all(cells[(h, "ref_snapshot")]["tokens_total"] <
              cells[(h, "raw_dom")]["tokens_total"] for h in HARNESSES))
    check("task ids match the fixture suite",
          {r["task_id"] for r in runs} == {t["id"] for t in tasks})

    table = render_tables(cells, HARNESSES, CONDITIONS)
    check("table lists every harness and condition",
          all(h in table for h in HARNESSES) and all(c in table for c in CONDITIONS))
    comparison = table.split("## raw DOM vs ref snapshots (per harness)")[1]
    check("comparison table has one row per harness",
          sum(1 for line in comparison.splitlines() if line.startswith("| ")) == len(HARNESSES) + 1)

    with tempfile.TemporaryDirectory(prefix="telemetry-bench-selftest-") as tmp:
        root = Path(tmp)
        doc = results_document(runs, "synthetic-self-test", DEFAULT_SEED)
        doc_path = root / "results.json"
        doc_path.write_text(json.dumps(doc), encoding="utf-8")
        loaded = load_results(doc_path)
        check("results JSON round-trips", aggregate(loaded["runs"]) == cells)

        template = json.loads(RESULTS_TEMPLATE.read_text(encoding="utf-8"))
        check("results template is empty and schema-matched",
              template["schema_version"] == SCHEMA_VERSION and template["runs"] == []
              and set(template["harnesses"]) == set(HARNESSES)
              and set(template["conditions"]) == set(CONDITIONS))

        # Telemetry confirmation with synthetic local rollups: only numeric
        # bucket codes; no page content is created, read or sent.
        telemetry_dir = root / "telemetry" / "consistent"
        telemetry_dir.mkdir(parents=True)
        bench_ratio = bucket_token_ratio(
            cells[(HARNESSES[0], "ref_snapshot")]["tokens_total"],
            cells[(HARNESSES[0], "raw_dom")]["tokens_total"])
        bench_ms = bucket_snapshot_ms(cells[(HARNESSES[0], "ref_snapshot")]["latency_median_ms"])
        ping = {
            "schema_version": SCHEMA_VERSION,
            "app_version": "synthetic",
            "distro": 0, "arch": 0, "install_id": "00000000-0000-0000-0000-000000000000",
            "agent": {
                "enabled": True, "harness": [HARNESS_CODES[h] for h in HARNESSES],
                "sessions": 1, "calls_by_action": {}, "snapshot_token_ratio": bench_ratio,
                "snapshot_ms": bench_ms, "element_count": 0, "ref_resolve_success": 9,
                "errors_by_code": {}, "plugins_installed": 0,
            },
        }
        (telemetry_dir / "rollup-1.json").write_text(json.dumps(ping), encoding="utf-8")
        confirmed = confirm_telemetry(cells, telemetry_dir, HARNESSES)
        check("matching field buckets report consistent",
              confirmed.count("consistent") >= 2 and "NOT RUN" not in confirmed)
        mismatch_dir = root / "telemetry" / "mismatch"
        mismatch_dir.mkdir()
        (mismatch_dir / "rollup-1.json").write_text(
            json.dumps({**ping, "agent": {**ping["agent"],
                                          "snapshot_token_ratio": (bench_ratio + 1) % 4,
                                          "snapshot_ms": (bench_ms + 1) % 4}}),
            encoding="utf-8")
        check("mismatched field buckets are informational",
              "outside observed bands" in confirm_telemetry(cells, mismatch_dir, HARNESSES)
              and aggregate(loaded["runs"]) == cells)
        check("absent telemetry is a no-op",
              "NOT RUN" in confirm_telemetry(cells, root / "missing", HARNESSES))

    print("=== TELEMETRY BENCH: OFFLINE SELF-TEST (synthetic data) ===")
    print(f"seed={DEFAULT_SEED} tasks={len(tasks)} cases={len(runs)} mode=synthetic")
    print()
    print(table)
    print()
    print(confirm_telemetry(cells, HERE / "no-telemetry-here", HARNESSES))
    for name, ok in checks:
        print(f"[{'PASS' if ok else 'FAIL'}] {name}")
    failed = [name for name, ok in checks if not ok]
    print(f"SELF-TEST {'PASS' if not failed else 'FAIL'} ({len(checks) - len(failed)}/{len(checks)} checks)")
    return 0 if not failed else 1


def main(argv=None) -> int:
    parser = argparse.ArgumentParser(description=__doc__,
                                     formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--self-test", action="store_true",
                        help="offline synthetic run; no browser, network, API keys or cargo")
    parser.add_argument("--results", type=Path, help="render a collected results JSON")
    parser.add_argument("--harness-cmd", help="opt-in command template driving an external harness")
    parser.add_argument("--fixtures", type=Path, default=FIXTURES_DIR, help="fixture directory")
    parser.add_argument("--tasks", type=int, default=0, help="limit task count (0 = all)")
    parser.add_argument("--seed", type=int, default=DEFAULT_SEED, help="synthetic seed")
    parser.add_argument("--confirm-telemetry", type=Path,
                        help="optional local rollup dir (confirmation only, never required)")
    parser.add_argument("--json-out", type=Path, help="write the results document as JSON")
    args = parser.parse_args(argv)

    tasks = list(BENCH_TASKS)
    if args.tasks:
        tasks = tasks[:args.tasks]

    if args.self_test:
        return self_test()

    if args.results and args.harness_cmd:
        parser.error("choose --results or --harness-cmd, not both")

    if args.results:
        doc = load_results(args.results)
        runs = doc["runs"]
        mode = doc.get("mode", "real-harness")
        seed = doc.get("seed")
    elif args.harness_cmd:
        if not args.fixtures.is_dir():
            parser.error(f"fixture directory not found: {args.fixtures}")
        runs = real_runs_from_command(args.harness_cmd, tasks, HARNESSES,
                                      CONDITIONS, args.fixtures)
        mode, seed = "real-harness", None
    else:
        parser.error("one of --self-test, --results or --harness-cmd is required")

    runs = [validate_case(r, r["harness"], r["condition"], r["task_id"]) for r in runs]
    cells = aggregate(runs)
    print(f"=== TELEMETRY BENCH ({mode}) ===")
    print(f"cases={len(runs)} tasks={len({r['task_id'] for r in runs})} seed={seed}")
    print()
    print(render_tables(cells, HARNESSES, CONDITIONS))
    if args.confirm_telemetry:
        print(confirm_telemetry(cells, args.confirm_telemetry, HARNESSES))
    else:
        print("Telemetry confirmation: NOT REQUESTED (optional; benchmark ran without telemetry).")
    if args.json_out:
        args.json_out.write_text(json.dumps(results_document(runs, mode, seed), indent=2),
                                 encoding="utf-8")
        print(f"Results written to {args.json_out}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
