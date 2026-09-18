export default {
  async scheduled(controller, env, ctx) {
    ctx.waitUntil(runMonitor(env));
  },

  async fetch(request, env) {
    const url = new URL(request.url);

    if (url.pathname === "/status" && request.method === "GET") {
      const latest = await env.TELEMETRY.get("monitor:latest", "json");

      return new Response(
        JSON.stringify(
          latest || {
            status: "UNKNOWN",
            message: "No monitor check has completed yet."
          }
        ),
        {
          status: 200,
          headers: {
            "Content-Type": "application/json",
            "Cache-Control": "no-store",
            "Access-Control-Allow-Origin": "*"
          }
        }
      );
    }

    return new Response("Daemon OS external monitor", {
      status: 200,
      headers: {
        "Content-Type": "text/plain",
        "Cache-Control": "no-store"
      }
    });
  }
};

async function runMonitor(env) {
  const base = null;
  const startedAt = Date.now();

  const targets = [
    {
      name: "homepage",
      url: `${base}/`
    },
    {
      name: "api-health",
      url: `${base}/api/health`
    }
  ];

  const results = [];

  for (const target of targets) {
    const checkStarted = Date.now();

    try {
      const response = await env.DAEMON_OS.fetch(new Request(target.url));

      const responseTimeMs = Date.now() - checkStarted;

      let body = null;

      if (target.name === "api-health") {
        try {
          body = await response.json();
        } catch {
          body = null;
        }
      } else {
        await response.text();
      }

      const healthy =
        response.status === 200 &&
        (target.name !== "api-health" || body?.ok === true);

      results.push({
        target: target.name,
        url: target.url,
        healthy,
        status_code: response.status,
        response_time_ms: responseTimeMs
      });
    } catch (error) {
      results.push({
        target: target.name,
        url: target.url,
        healthy: false,
        status_code: null,
        response_time_ms: Date.now() - checkStarted,
        error: error instanceof Error ? error.message : String(error)
      });
    }
  }

  const allHealthy = results.every((result) => result.healthy);

  const previous =
    (await env.TELEMETRY.get("monitor:latest", "json")) || {};

  const consecutiveFailures = allHealthy
    ? 0
    : Number(previous.consecutive_failures || 0) + 1;

  let status;

  if (allHealthy) {
    status = "OPERATIONAL";
  } else if (consecutiveFailures >= 3) {
    status = "OUTAGE";
  } else {
    status = "CHECKING";
  }

  const record = {
    status,
    checked_at: new Date().toISOString(),
    check_duration_ms: Date.now() - startedAt,
    consecutive_failures: consecutiveFailures,
    targets: results,
    previous_status: previous.status || null
  };

  await env.TELEMETRY.put(
    "monitor:latest",
    JSON.stringify(record),
    {
      expirationTtl: 60 * 60 * 24 * 30
    }
  );

  const historyKey =
    `monitor:history:${Date.now()}`;

  await env.TELEMETRY.put(
    historyKey,
    JSON.stringify(record),
    {
      expirationTtl: 60 * 60 * 24 * 30
    }
  );
}


