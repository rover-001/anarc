/** Types for the dependency-free validation.mjs module. */
export interface ActionCounts {
  click: number;
  ty: number;
  navigate: number;
  read: number;
  other: number;
}

export interface ErrorCounts {
  stale_ref: number;
  not_found: number;
  timeout: number;
  other: number;
}

export interface AgentStats {
  enabled: boolean;
  harness: number[];
  sessions: number;
  calls_by_action: ActionCounts;
  snapshot_token_ratio: number;
  snapshot_ms: number;
  element_count: number;
  ref_resolve_success: number;
  errors_by_code: ErrorCounts;
  plugins_installed: number;
}

export interface Ping {
  schema_version: number;
  app_version: string;
  distro: number;
  arch: number;
  install_id: string;
  agent: AgentStats | null;
}

export type ValidationResult =
  | { ok: true; value: Ping }
  | { ok: false; error: string };

export const MAX_BODY_BYTES: number;
export const SCHEMA_VERSION: number;
export function byteLength(body: string): number;
export function checkBodySize(body: string): boolean;
export function validatePing(value: unknown): ValidationResult;
export function parsePingBody(body: string): ValidationResult;
