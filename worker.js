export default {
  async fetch(request, env) {
    const url = new URL(request.url);

    // CORS
    const corsHeaders = {
      "Access-Control-Allow-Origin": "*",
      "Access-Control-Allow-Methods": "GET, POST, OPTIONS",
      "Access-Control-Allow-Headers": "Content-Type, Authorization"
    };

    if (request.method === "OPTIONS") {
      return new Response(null, {
        status: 204,
        headers: corsHeaders
      });
    }

    // Production health check
    if (url.pathname === "/api/health" && request.method === "GET") {
      return Response.json(
        {
          ok: true,
          service: "daemon-os",
          status: "operational",
          timestamp: new Date().toISOString()
        },
        { headers: corsHeaders }
      );
    }

    // Authenticated telemetry heartbeat
    if (url.pathname === "/api/heartbeat" && request.method === "POST") {
      const auth = request.headers.get("Authorization");

      if (!auth || !auth.startsWith("Bearer ")) {
        return Response.json(
          { error: "Unauthorized" },
          { status: 401, headers: corsHeaders }
        );
      }

      const token = auth.slice(7).trim();
      const configuredToken = (env.DAEMON_INGEST_TOKEN || "").trim();

      if (!configuredToken || token !== configuredToken) {
        return Response.json(
          { error: "Unauthorized" },
          { status: 401, headers: corsHeaders }
        );
      }

      let body;

      try {
        body = await request.json();
      } catch {
        return Response.json(
          { error: "Invalid JSON" },
          { status: 400, headers: corsHeaders }
        );
      }

      const deviceId = body.device_id ?? "unknown";
      const receivedAt = new Date().toISOString();

      await env.TELEMETRY.put(
        `device:${deviceId}`,
        JSON.stringify({
          ...body,
          received_at: receivedAt
        })
      );

      return Response.json(
        {
          ok: true,
          received: true,
          timestamp: receivedAt,
          device_id: deviceId
        },
        { headers: corsHeaders }
      );
    }

    // Everything else → Daemon frontend
    return env.ASSETS.fetch(request);
  }
};
