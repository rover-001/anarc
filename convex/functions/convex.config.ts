import { defineApp } from "convex/server";
import { v } from "convex/values";
import rateLimiter from "@convex-dev/rate-limiter/convex.config.js";

// Registers the rate-limiter component. Run `npx convex dev` (or `deploy`)
// once with this file present; Convex installs the component automatically.
// Env declarations also make `env` from _generated/server type-safe.
const app = defineApp({
  env: {
    TELEMETRY_ADMIN_TOKEN: v.optional(v.string()),
    TELEMETRY_GLOBAL_DAILY_LIMIT: v.optional(v.string()),
  },
});
app.use(rateLimiter);

export default app;
