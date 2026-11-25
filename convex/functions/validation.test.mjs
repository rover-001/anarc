/**
 * node --test convex/validation.test.mjs
 *
 * These tests import only the portable validator. No Convex runtime, no
 * network, no database, no Node-specific code inside validation.mjs.
 */
import test from "node:test";
import assert from "node:assert/strict";
import { MAX_BODY_BYTES, byteLength, checkBodySize, parsePingBody, validatePing } from "./validation.mjs";

function validPing() {
  return {
    schema_version: 1,
    app_version: "0.1.0",
    distro: 0,
    arch: 0,
    install_id: "550e8400-e29b-41d4-a716-446655440000",
    agent: {
      enabled: true,
      harness: [0, 1],
      sessions: 4,
      calls_by_action: { click: 2, ty: 0, navigate: 1, read: 9, other: 4 },
      snapshot_token_ratio: 3,
      snapshot_ms: 2,
      element_count: 1,
      ref_resolve_success: 9,
      errors_by_code: { stale_ref: 1, not_found: 0, timeout: 0, other: 3 },
      plugins_installed: 0,
    },
  };
}

function clone(value) {
  return JSON.parse(JSON.stringify(value));
}

function expectRejectedWith(value, code) {
  const result = validatePing(value);
  assert.equal(result.ok, false, JSON.stringify(value));
  assert.match(result.error, new RegExp(`^${code}:`));
}

test("valid payload accepted and normalized", () => {
  const result = parsePingBody(JSON.stringify(validPing()));
  assert.equal(result.ok, true);
  assert.deepEqual(result.value, validPing());
});

test("agent:null is valid and all required fields still enforced", () => {
  const ping = validPing();
  ping.agent = null;
  assert.equal(validatePing(ping).ok, true);
  for (const key of ["schema_version", "app_version", "distro", "arch", "install_id", "agent"]) {
    const missing = clone(ping);
    delete missing[key];
    expectRejectedWith(missing, "missing_field");
  }
});

test("unknown fields rejected at every object level", () => {
  const top = validPing();
  top.url = "https://example.com";
  expectRejectedWith(top, "unknown_field");

  const agent = validPing();
  agent.agent.prompt = "secret";
  expectRejectedWith(agent, "unknown_field");

  const actions = validPing();
  actions.agent.calls_by_action.host = "example.com";
  expectRejectedWith(actions, "unknown_field");

  const errors = validPing();
  errors.agent.errors_by_code.message = "oops";
  expectRejectedWith(errors, "unknown_field");

  // Prototype-looking keys are just unknown fields.
  const polluted = JSON.parse(JSON.stringify(validPing()).replace('"agent":', '"__proto__":{},"agent":'));
  expectRejectedWith(polluted, "unknown_field");
});

test("oversize bodies rejected before parsing", () => {
  assert.equal(MAX_BODY_BYTES, 4096);
  assert.equal(byteLength("a".repeat(4096)), 4096);
  assert.equal(checkBodySize("a".repeat(4096)), true);
  assert.equal(checkBodySize("a".repeat(4097)), false);
  const oversize = parsePingBody(JSON.stringify(validPing()) + " ".repeat(4097));
  assert.deepEqual(oversize, { ok: false, error: "body_too_large" });
  // Multi-byte characters are counted as UTF-8 bytes, not UTF-16 units.
  assert.equal(checkBodySize("é".repeat(2048)), true);
  assert.equal(checkBodySize("é".repeat(2049)), false);
  assert.deepEqual(parsePingBody("{bad json"), { ok: false, error: "invalid_json" });
});

test("malformed and non-v4 install ids rejected", () => {
  const bad = [
    "not-a-uuid",
    "550e8400e29b41d4a716446655440000",
    "550e8400-e29b-11d4-a716-446655440000", // version 1
    "550e8400-e29b-41d4-c716-446655440000", // bad variant
    "550e8400-e29b-41d4-a716-44665544000", // too short
    42,
  ];
  for (const installId of bad) {
    const ping = validPing();
    ping.install_id = installId;
    expectRejectedWith(ping, "invalid_uuid");
  }
  const upper = validPing();
  upper.install_id = upper.install_id.toUpperCase();
  assert.equal(validatePing(upper).ok, true);
});

test("out-of-range enum and bucket codes rejected", () => {
  const cases = [
    ["distro", 6, "invalid_enum"],
    ["distro", -1, "invalid_enum"],
    ["arch", 2, "invalid_enum"],
    ["schema_version", 2, "invalid_schema_version"],
  ];
  for (const [key, value, code] of cases) {
    const ping = validPing();
    ping[key] = value;
    expectRejectedWith(ping, code);
  }

  const bucketCases = [
    ["sessions", 5],
    ["snapshot_token_ratio", 4],
    ["snapshot_ms", 4],
    ["element_count", 4],
    ["ref_resolve_success", 10],
    ["plugins_installed", 10],
  ];
  for (const [key, value] of bucketCases) {
    const ping = validPing();
    ping.agent[key] = value;
    expectRejectedWith(ping, "invalid_bucket");
  }
  for (const key of ["click", "ty", "navigate", "read", "other"]) {
    const ping = validPing();
    ping.agent.calls_by_action[key] = 10;
    expectRejectedWith(ping, "invalid_bucket");
  }
  for (const key of ["stale_ref", "not_found", "timeout", "other"]) {
    const ping = validPing();
    ping.agent.errors_by_code[key] = -1;
    expectRejectedWith(ping, "invalid_bucket");
  }
  const harness = validPing();
  harness.agent.harness = [4];
  expectRejectedWith(harness, "invalid_enum");
  const fractional = validPing();
  fractional.agent.sessions = 1.5;
  expectRejectedWith(fractional, "invalid_bucket");
});

test("wrong types rejected", () => {
  for (const [key, value, code] of [
    ["app_version", 12, "invalid_type"],
    ["schema_version", "1", "invalid_schema_version"],
    ["agent", "enabled", "invalid_type"],
    ["distro", "0", "invalid_enum"],
  ]) {
    const ping = validPing();
    ping[key] = value;
    expectRejectedWith(ping, code);
  }
  const agent = validPing();
  agent.agent.enabled = "yes";
  expectRejectedWith(agent, "invalid_type");
  const harness = validPing();
  harness.agent.harness = "0";
  expectRejectedWith(harness, "invalid_type");
});
