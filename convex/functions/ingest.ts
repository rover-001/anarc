import { v } from "convex/values";
import { internalMutation } from "./_generated/server";
import { rateLimiter } from "./rateLimiter";

// Mirrors the portable validator in validation.mjs. Convex validates again at
// the function boundary; the HTTP action rejects malformed bodies first.
const pingValidator = v.object({
  schema_version: v.number(),
  app_version: v.string(),
  distro: v.number(),
  arch: v.number(),
  install_id: v.string(),
  agent: v.union(
    v.null(),
    v.object({
      enabled: v.boolean(),
      harness: v.array(v.number()),
      sessions: v.number(),
      calls_by_action: v.object({
        click: v.number(),
        ty: v.number(),
        navigate: v.number(),
        read: v.number(),
        other: v.number(),
      }),
      snapshot_token_ratio: v.number(),
      snapshot_ms: v.number(),
      element_count: v.number(),
      ref_resolve_success: v.number(),
      errors_by_code: v.object({
        stale_ref: v.number(),
        not_found: v.number(),
        timeout: v.number(),
        other: v.number(),
      }),
      plugins_installed: v.number(),
    }),
  ),
});

const ACTION_KEYS = ["click", "ty", "navigate", "read", "other"] as const;
const ERROR_KEYS = ["stale_ref", "not_found", "timeout", "other"] as const;

function zeros(length: number): number[] {
  return Array.from({ length }, () => 0);
}

/** Increment one bucket in a fixed-length histogram (defensive on bad data). */
function bump(hist: number[] | undefined, code: number, length: number): number[] {
  const next = hist && hist.length === length ? hist.slice() : zeros(length);
  next[code] = (next[code] ?? 0) + 1;
  return next;
}

function countActivity(counts: { click: number; ty: number; navigate: number; read: number; other: number } | undefined, raw: { click: number; ty: number; navigate: number; read: number; other: number }) {
  const next = counts ? { ...counts } : { click: 0, ty: 0, navigate: 0, read: 0, other: 0 };
  for (const key of ACTION_KEYS) if (raw[key] > 0) next[key] += 1;
  return next;
}

function countErrors(counts: { staleRef: number; notFound: number; timeout: number; other: number } | undefined, raw: { stale_ref: number; not_found: number; timeout: number; other: number }) {
  const next = counts ? { ...counts } : { staleRef: 0, notFound: 0, timeout: 0, other: 0 };
  if (raw.stale_ref > 0) next.staleRef += 1;
  if (raw.not_found > 0) next.notFound += 1;
  if (raw.timeout > 0) next.timeout += 1;
  if (raw.other > 0) next.other += 1;
  return next;
}

/** The only write path. All rate limits and rollups commit in one transaction. */
export const recordPing = internalMutation({
  args: { ping: pingValidator, day: v.string() },
  handler: async (ctx, { ping, day }) => {
    // Check the global cap without consuming; then consume both limits.
    // Convex mutations are transactional, so concurrent pings cannot overshoot.
    const globalStatus = await rateLimiter.check(ctx, "globalPing");
    if (!globalStatus.ok) return { ok: false as const, reason: "global_rate_limit" };

    const installStatus = await rateLimiter.limit(ctx, "pingPerInstall", { key: ping.install_id });
    if (!installStatus.ok) return { ok: false as const, reason: "install_rate_limit" };

    const globalConsumed = await rateLimiter.limit(ctx, "globalPing");
    if (!globalConsumed.ok) return { ok: false as const, reason: "global_rate_limit" };

    const install = await ctx.db
      .query("installs")
      .withIndex("by_install", (q) => q.eq("installId", ping.install_id))
      .unique();
    if (!install) {
      await ctx.db.insert("installs", { installId: ping.install_id, firstSeen: day, lastSeen: day });
    } else if (install.lastSeen < day) {
      await ctx.db.patch(install._id, { lastSeen: day });
    }

    const daily = await ctx.db
      .query("daily")
      .withIndex("by_day_version_distro_arch", (q) =>
        q.eq("day", day).eq("version", ping.app_version).eq("distro", ping.distro).eq("arch", ping.arch),
      )
      .unique();
    const agent = ping.agent;
    const dailyFields = {
      pings: (daily?.pings ?? 0) + 1,
      agentPings: (daily?.agentPings ?? 0) + (agent ? 1 : 0),
      calls: countActivity(daily?.calls, agent?.calls_by_action ?? { click: 0, ty: 0, navigate: 0, read: 0, other: 0 }),
      errors: countErrors(daily?.errors, agent?.errors_by_code ?? { stale_ref: 0, not_found: 0, timeout: 0, other: 0 }),
    };
    if (daily) {
      await ctx.db.patch(daily._id, dailyFields);
    } else {
      await ctx.db.insert("daily", {
        day,
        version: ping.app_version,
        distro: ping.distro,
        arch: ping.arch,
        ...dailyFields,
      });
    }

    if (agent) {
      // Deduplicate harness codes: each ping contributes once per harness.
      for (const harness of new Set(agent.harness)) {
        const row = await ctx.db
          .query("agent_daily")
          .withIndex("by_day_harness", (q) => q.eq("day", day).eq("harness", harness))
          .unique();
        const fields = {
          pings: (row?.pings ?? 0) + 1,
          sessions: bump(row?.sessions, agent.sessions, 5),
          callsByAction: {
            click: bump(row?.callsByAction?.click, agent.calls_by_action.click, 10),
            ty: bump(row?.callsByAction?.ty, agent.calls_by_action.ty, 10),
            navigate: bump(row?.callsByAction?.navigate, agent.calls_by_action.navigate, 10),
            read: bump(row?.callsByAction?.read, agent.calls_by_action.read, 10),
            other: bump(row?.callsByAction?.other, agent.calls_by_action.other, 10),
          },
          snapshotTokenRatio: bump(row?.snapshotTokenRatio, agent.snapshot_token_ratio, 4),
          snapshotMs: bump(row?.snapshotMs, agent.snapshot_ms, 4),
          elementCount: bump(row?.elementCount, agent.element_count, 4),
          refResolveSuccess: bump(row?.refResolveSuccess, agent.ref_resolve_success, 10),
          errorsByCode: {
            staleRef: bump(row?.errorsByCode?.staleRef, agent.errors_by_code.stale_ref, 10),
            notFound: bump(row?.errorsByCode?.notFound, agent.errors_by_code.not_found, 10),
            timeout: bump(row?.errorsByCode?.timeout, agent.errors_by_code.timeout, 10),
            other: bump(row?.errorsByCode?.other, agent.errors_by_code.other, 10),
          },
          pluginsInstalled: bump(row?.pluginsInstalled, agent.plugins_installed, 10),
        };
        if (row) {
          await ctx.db.patch(row._id, fields);
        } else {
          await ctx.db.insert("agent_daily", { day, harness, ...fields });
        }
      }
    }

    return { ok: true as const };
  },
});
