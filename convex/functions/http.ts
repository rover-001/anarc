import { httpRouter } from "convex/server";
import { internal } from "./_generated/api";
import { httpAction } from "./_generated/server";
import { MAX_BODY_BYTES, parsePingBody } from "./validation.mjs";

const http = httpRouter();

function json(body: object, status: number): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "content-type": "application/json", "cache-control": "no-store" },
  });
}

http.route({
  path: "/ping",
  method: "POST",
  handler: httpAction(async (ctx, request) => {
    // The only header ever read is content-length, solely to reject oversized
    // bodies early. No IP, user-agent, referer or any other header is inspected
    // or stored. The authoritative check runs on the body itself.
    const declared = request.headers.get("content-length");
    if (declared) {
      const size = Number(declared);
      if (Number.isFinite(size) && size > MAX_BODY_BYTES) return json({ error: "body_too_large" }, 413);
    }

    const parsed = parsePingBody(await request.text());
    if (!parsed.ok) return json({ error: parsed.error }, 400);

    // Server-side UTC day only; no exact timestamp is kept.
    const day = new Date().toISOString().slice(0, 10);
    const result = await ctx.runMutation(internal.ingest.recordPing, { ping: parsed.value, day });
    if (!result.ok) return json({ error: result.reason }, 429);

    return json({ ok: true }, 200);
  }),
});

export default http;
