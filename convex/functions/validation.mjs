/**
 * Portable wire validation for Anarc telemetry pings.
 *
 * Deliberately dependency-free (no Convex imports, no Node built-ins): the same
 * module is used by the Convex HTTP action and by `node --test`, and can be
 * lifted into an Axum + Postgres port without changes. Errors are fixed codes,
 * never echoes of request data.
 *
 * Source of truth: TELEMETRY.md and crates/anarc-telemetry/src/{schema,buckets}.rs.
 */

export const MAX_BODY_BYTES = 4096;
export const SCHEMA_VERSION = 1;

const DISTRO_MAX = 5; // arch=0 ubuntu=1 fedora=2 debian=3 nixos=4 other=5
const ARCH_MAX = 1; // x86_64=0 aarch64=1
const HARNESS_MAX = 3; // claude=0 codex=1 antigravity=2 other=3
const BUCKET_MAX = 9; // widest band set: ref-resolve success (0..9)
const APP_VERSION_MAX_CHARS = 128; // bounded even though it is a free-form string

// UUID v4 with RFC 4122 variant bits (8/9/a/b).
const UUID_V4 = /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i;

const PING_KEYS = ["schema_version", "app_version", "distro", "arch", "install_id", "agent"];
const AGENT_KEYS = [
  "enabled",
  "harness",
  "sessions",
  "calls_by_action",
  "snapshot_token_ratio",
  "snapshot_ms",
  "element_count",
  "ref_resolve_success",
  "errors_by_code",
  "plugins_installed",
];
const ACTION_KEYS = ["click", "ty", "navigate", "read", "other"];
const ERROR_KEYS = ["stale_ref", "not_found", "timeout", "other"];

function fail(code, path) {
  return { ok: false, error: path ? `${code}:${path}` : code };
}

function has(obj, key) {
  return Object.prototype.hasOwnProperty.call(obj, key);
}

/** Reject both extra and missing keys: `deny_unknown_fields` at every level. */
function exactKeys(value, keys, path) {
  if (value === null || typeof value !== "object" || Array.isArray(value)) {
    return fail("invalid_type", path);
  }
  for (const key of Object.keys(value)) {
    if (!keys.includes(key)) return fail("unknown_field", `${path}.${key}`);
  }
  for (const key of keys) {
    if (!has(value, key)) return fail("missing_field", `${path}.${key}`);
  }
  return null;
}

function intInRange(value, min, max, code, path) {
  if (!Number.isInteger(value) || value < min || value > max) return fail(code, path);
  return null;
}

/** Bucket codes are integers; narrower bands are enforced per field. */
function bucket(value, max, path) {
  return intInRange(value, 0, max, "invalid_bucket", path);
}

/** Exact UTF-8 byte count; body is capped before JSON.parse. */
export function byteLength(body) {
  if (typeof body !== "string") return -1;
  let bytes = 0;
  for (let i = 0; i < body.length; i += 1) {
    const unit = body.charCodeAt(i);
    if (unit < 0x80) {
      bytes += 1;
    } else if (unit < 0x800) {
      bytes += 2;
    } else if (
      unit >= 0xd800 &&
      unit <= 0xdbff &&
      i + 1 < body.length &&
      body.charCodeAt(i + 1) >= 0xdc00 &&
      body.charCodeAt(i + 1) <= 0xdfff
    ) {
      bytes += 4;
      i += 1;
    } else {
      bytes += 3;
    }
  }
  return bytes;
}

export function checkBodySize(body) {
  const bytes = byteLength(body);
  return bytes >= 0 && bytes <= MAX_BODY_BYTES;
}

function validateActionCounts(value, path) {
  const shape = exactKeys(value, ACTION_KEYS, path);
  if (shape) return shape;
  const out = {};
  for (const key of ACTION_KEYS) {
    const error = bucket(value[key], BUCKET_MAX, `${path}.${key}`);
    if (error) return error;
    out[key] = value[key];
  }
  return { ok: true, value: out };
}

function validateErrorCounts(value, path) {
  const shape = exactKeys(value, ERROR_KEYS, path);
  if (shape) return shape;
  const out = {};
  for (const key of ERROR_KEYS) {
    const error = bucket(value[key], BUCKET_MAX, `${path}.${key}`);
    if (error) return error;
    out[key] = value[key];
  }
  return { ok: true, value: out };
}

function validateAgent(value, path) {
  const shape = exactKeys(value, AGENT_KEYS, path);
  if (shape) return shape;
  if (typeof value.enabled !== "boolean") return fail("invalid_type", `${path}.enabled`);
  if (!Array.isArray(value.harness)) return fail("invalid_type", `${path}.harness`);
  for (let i = 0; i < value.harness.length; i += 1) {
    const error = intInRange(value.harness[i], 0, HARNESS_MAX, "invalid_enum", `${path}.harness[${i}]`);
    if (error) return error;
  }

  const bands = [
    [value.sessions, 4, "sessions"],
    [value.snapshot_token_ratio, 3, "snapshot_token_ratio"],
    [value.snapshot_ms, 3, "snapshot_ms"],
    [value.element_count, 3, "element_count"],
    [value.ref_resolve_success, BUCKET_MAX, "ref_resolve_success"],
    [value.plugins_installed, BUCKET_MAX, "plugins_installed"],
  ];
  const simple = {};
  for (const [raw, max, key] of bands) {
    const error = bucket(raw, max, `${path}.${key}`);
    if (error) return error;
    simple[key] = raw;
  }

  const actions = validateActionCounts(value.calls_by_action, `${path}.calls_by_action`);
  if (!actions.ok) return actions;
  const errors = validateErrorCounts(value.errors_by_code, `${path}.errors_by_code`);
  if (!errors.ok) return errors;

  return {
    ok: true,
    value: {
      enabled: value.enabled,
      harness: value.harness.slice(),
      sessions: simple.sessions,
      calls_by_action: actions.value,
      snapshot_token_ratio: simple.snapshot_token_ratio,
      snapshot_ms: simple.snapshot_ms,
      element_count: simple.element_count,
      ref_resolve_success: simple.ref_resolve_success,
      errors_by_code: errors.value,
      plugins_installed: simple.plugins_installed,
    },
  };
}

/** Validate one parsed JSON value against the fixed schema. */
export function validatePing(value) {
  const shape = exactKeys(value, PING_KEYS, "$");
  if (shape) return shape;
  if (value.schema_version !== SCHEMA_VERSION) return fail("invalid_schema_version", "$.schema_version");
  if (typeof value.app_version !== "string" || value.app_version.length === 0 || value.app_version.length > APP_VERSION_MAX_CHARS) {
    return fail("invalid_type", "$.app_version");
  }
  let error = intInRange(value.distro, 0, DISTRO_MAX, "invalid_enum", "$.distro");
  if (error) return error;
  error = intInRange(value.arch, 0, ARCH_MAX, "invalid_enum", "$.arch");
  if (error) return error;
  if (typeof value.install_id !== "string" || !UUID_V4.test(value.install_id)) {
    return fail("invalid_uuid", "$.install_id");
  }

  let agent = null;
  if (value.agent !== null) {
    const result = validateAgent(value.agent, "$.agent");
    if (!result.ok) return result;
    agent = result.value;
  }

  return {
    ok: true,
    value: {
      schema_version: value.schema_version,
      app_version: value.app_version,
      distro: value.distro,
      arch: value.arch,
      install_id: value.install_id,
      agent,
    },
  };
}

/** Cap, parse and validate the raw request body in one portable call. */
export function parsePingBody(body) {
  if (!checkBodySize(body)) return fail("body_too_large");
  let parsed;
  try {
    parsed = JSON.parse(body);
  } catch {
    return fail("invalid_json");
  }
  return validatePing(parsed);
}
