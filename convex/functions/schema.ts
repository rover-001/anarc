import { defineSchema, defineTable } from "convex/server";
import { v } from "convex/values";

// Only counts, day strings, enum codes and bucket histograms are stored.
// No page strings, no IPs, no exact timestamps, no install id in rollup rows.

// Bucket histograms are fixed-length arrays; lengths are enforced in ingest.ts.
// sessions 0..4 (5 slots); token ratio / snapshot ms / element count 0..3 (4);
// calls, errors, ref-resolve and plugins 0..9 (10).
const histogram = v.array(v.number());

export default defineSchema({
  daily: defineTable({
    day: v.string(), // YYYY-MM-DD, UTC
    version: v.string(), // app_version from Cargo
    distro: v.number(),
    arch: v.number(),
    pings: v.number(),
    agentPings: v.number(),
    calls: v.object({
      click: v.number(),
      ty: v.number(),
      navigate: v.number(),
      read: v.number(),
      other: v.number(),
    }),
    errors: v.object({
      staleRef: v.number(),
      notFound: v.number(),
      timeout: v.number(),
      other: v.number(),
    }),
  })
    .index("by_day_version_distro_arch", ["day", "version", "distro", "arch"])
    .index("by_day", ["day"]),

  agent_daily: defineTable({
    day: v.string(),
    harness: v.number(), // enum code 0..3
    pings: v.number(),
    sessions: histogram, // length 5
    callsByAction: v.object({
      click: histogram,
      ty: histogram,
      navigate: histogram,
      read: histogram,
      other: histogram,
    }),
    snapshotTokenRatio: histogram, // length 4
    snapshotMs: histogram, // length 4
    elementCount: histogram, // length 4
    refResolveSuccess: histogram, // length 10
    errorsByCode: v.object({
      staleRef: histogram,
      notFound: histogram,
      timeout: histogram,
      other: histogram,
    }),
    pluginsInstalled: histogram, // length 10
  })
    .index("by_day_harness", ["day", "harness"])
    .index("by_day", ["day"]),

  installs: defineTable({
    installId: v.string(), // UUID v4, key
    firstSeen: v.string(), // YYYY-MM-DD, UTC
    lastSeen: v.string(), // YYYY-MM-DD, UTC
  })
    .index("by_install", ["installId"])
    .index("by_last_seen", ["lastSeen"]),
});
