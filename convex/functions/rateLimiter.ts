import { DAY, RateLimiter } from "@convex-dev/rate-limiter";
import { components } from "./_generated/api";
import { env } from "./_generated/server";

// Fixed windows anchored at epoch 0 => UTC midnight boundaries.
// `start: 0` is intentional; omitting it gives each install a random offset.
const fixedUtcDay = (rate: number) => ({ kind: "fixed window" as const, rate, period: DAY, start: 0 });

const parsed = Number(env.TELEMETRY_GLOBAL_DAILY_LIMIT ?? "100000");
export const GLOBAL_DAILY_LIMIT = Number.isFinite(parsed) && parsed > 0 ? Math.floor(parsed) : 100000;

export const rateLimiter = new RateLimiter(components.rateLimiter, {
  // One ping per install_id per UTC day.
  pingPerInstall: fixedUtcDay(1),
  // Global daily ceiling; bounds abuse from many rotating install ids.
  globalPing: fixedUtcDay(GLOBAL_DAILY_LIMIT),
});
