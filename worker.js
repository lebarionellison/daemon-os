export default {
  async fetch(request, env) {
    const url = new URL(request.url);

    if (url.pathname === "/api/debug-auth") {
      const auth = request.headers.get("Authorization");
      const incoming = auth && auth.startsWith("Bearer ") ? auth.slice(7) : "";
      return Response.json({
        secret_configured: !!env.DAEMON_INGEST_TOKEN,
        incoming_length: incoming.length,
        configured_length: env.DAEMON_INGEST_TOKEN ? env.DAEMON_INGEST_TOKEN.length : 0
      });
    }

    if (url.pathname === "/api/heartbeat" && request.method === "POST") {
      const auth = request.headers.get("Authorization");

      if (!auth || !auth.startsWith("Bearer ")) {
        return new Response("Unauthorized", { status: 401 });
      }

      const token = auth.slice(7);

      if (token !== env.DAEMON_INGEST_TOKEN) {
        return new Response("Unauthorized", { status: 401 });
      }

      const body = await request.json();

      await env.TELEMETRY.put(
        `device:${body.device_id ?? "unknown"}`,
        JSON.stringify({
          ...body,
          received_at: new Date().toISOString()
        })
      );

      return Response.json({
        ok: true,
        received: true,
        timestamp: new Date().toISOString(),
        device_id: body.device_id ?? null
      });
    }

    return env.ASSETS.fetch(request);
  }
};
