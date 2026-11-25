import { v } from "convex/values";
import { env, query } from "./_generated/server";

// Minimal auth gate: Convex Auth helpers are not wired in this repository, so
// the dashboard requires a shared admin token from the deployment environment.
// See convex/README.md for setup. The token is never logged or stored.
function tokenEquals(supplied: string, expected: string): boolean {
  if (supplied.length !== expected.length) return false;
  let diff = 0;
  for (let i = 0; i < supplied.length; i += 1) diff |= supplied.charCodeAt(i) ^ expected.charCodeAt(i);
  return diff === 0;
}

function dayBefore(day: string, days: number): string {
  const date = new Date(`${day}T00:00:00Z`);
  date.setUTCDate(date.getUTCDate() - days);
  return date.toISOString().slice(0, 10);
}

function addHistogram(target: number[], source: number[]): void {
  for (let i = 0; i < source.length; i += 1) target[i] = (target[i] ?? 0) + (source[i] ?? 0);
}

function medianBucket(histogram: number[]): number | null {
  let total = 0;
  for (const count of histogram) total += count;
  if (total === 0) return null;
  let remaining = total / 2; // lower median for even totals
  for (let i = 0; i < histogram.length; i += 1) {
    remaining -= histogram[i] ?? 0;
    if (remaining <= 0) return i;
  }
  return histogram.length - 1;
}

function ratePer1k(count: number, pings: number): number {
  return pings === 0 ? 0 : Math.round((count * 1000 * 100) / pings) / 100;
}

/** Auth-gated rollup view. Returns only aggregate counts derived from pings. */
export const summary = query({
  args: { adminToken: v.string(), rangeDays: v.optional(v.number()) },
  handler: async (ctx, args) => {
    const expected = env.TELEMETRY_ADMIN_TOKEN;
    if (!expected || !tokenEquals(args.adminToken, expected)) throw new Error("Unauthorized");

    const rangeDays = Math.min(365, Math.max(1, Math.floor(args.rangeDays ?? 30)));
    const today = new Date().toISOString().slice(0, 10);
    const weekAgo = dayBefore(today, 6);
    const monthAgo = dayBefore(today, rangeDays - 1);

    // DAU/WAU/MAU: distinct installs whose lastSeen falls in each window.
    const installs = await ctx.db
      .query("installs")
      .withIndex("by_last_seen", (q) => q.gte("lastSeen", monthAgo))
      .collect();
    const dau = installs.filter((row) => row.lastSeen === today).length;
    const wau = installs.filter((row) => row.lastSeen >= weekAgo).length;
    const mau = installs.length;

    const dailyRows = await ctx.db.query("daily").withIndex("by_day", (q) => q.gte("day", monthAgo)).collect();
    let totalPings = 0;
    let agentPings = 0;
    const errors = { staleRef: 0, notFound: 0, timeout: 0, other: 0 };
    const calls = { click: 0, ty: 0, navigate: 0, read: 0, other: 0 };
    const versionDistro = new Map<string, number>();
    for (const row of dailyRows) {
      totalPings += row.pings;
      agentPings += row.agentPings;
      errors.staleRef += row.errors.staleRef;
      errors.notFound += row.errors.notFound;
      errors.timeout += row.errors.timeout;
      errors.other += row.errors.other;
      for (const key of ["click", "ty", "navigate", "read", "other"] as const) calls[key] += row.calls[key];
      const key = `${row.version}|${row.distro}`;
      versionDistro.set(key, (versionDistro.get(key) ?? 0) + row.pings);
    }

    const versionSplit = [...versionDistro.entries()]
      .map(([key, pings]) => {
        const separator = key.lastIndexOf("|");
        return {
          version: key.slice(0, separator),
          distro: Number(key.slice(separator + 1)),
          pings,
          sharePct: totalPings === 0 ? 0 : Math.round((pings * 10000) / totalPings) / 100,
        };
      })
      .sort((a, b) => b.pings - a.pings);

    const agentRows = await ctx.db.query("agent_daily").withIndex("by_day", (q) => q.gte("day", monthAgo)).collect();
    const harnessPings = new Map<number, number>();
    const tokenRatio = [0, 0, 0, 0];
    const snapshotMs = [0, 0, 0, 0];
    for (const row of agentRows) {
      harnessPings.set(row.harness, (harnessPings.get(row.harness) ?? 0) + row.pings);
      addHistogram(tokenRatio, row.snapshotTokenRatio);
      addHistogram(snapshotMs, row.snapshotMs);
    }

    const harnessTotal = [...harnessPings.values()].reduce((sum, pings) => sum + pings, 0);
    const harnessSplit = [...harnessPings.entries()]
      .map(([harness, pings]) => ({
        harness,
        pings,
        sharePct: harnessTotal === 0 ? 0 : Math.round((pings * 10000) / harnessTotal) / 100,
      }))
      .sort((a, b) => b.pings - a.pings);

    return {
      window: { rangeDays, from: monthAgo, to: today },
      activity: {
        dau,
        wau,
        mau,
        pings: totalPings,
      },
      agent: {
        adoptionPct: totalPings === 0 ? 0 : Math.round((agentPings * 10000) / totalPings) / 100,
        harnessSplit,
        medianTokenRatioBucket: medianBucket(tokenRatio),
        tokenRatioBuckets: tokenRatio,
        medianSnapshotMsBucket: medianBucket(snapshotMs),
        snapshotMsBuckets: snapshotMs,
      },
      versionSplit,
      calls,
      errors: {
        counts: errors,
        per1kPings: {
          staleRef: ratePer1k(errors.staleRef, totalPings),
          notFound: ratePer1k(errors.notFound, totalPings),
          timeout: ratePer1k(errors.timeout, totalPings),
          other: ratePer1k(errors.other, totalPings),
        },
      },
    };
  },
});
