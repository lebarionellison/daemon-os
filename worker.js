export default {
  async fetch(request, env) {
    const url = new URL(request.url);

    const allowedOrigins = new Set([
      "https://daemon-os.lebarionellison.workers.dev",
      "https://lebarionellison.github.io",
      "http://127.0.0.1:8080",
      "http://localhost:8080"
    ]);

    const requestOrigin = request.headers.get("Origin");
    const origin = allowedOrigins.has(requestOrigin)
      ? requestOrigin
      : "https://daemon-os.lebarionellison.workers.dev";

    const corsHeaders = {
      "Access-Control-Allow-Origin": origin,
      "Access-Control-Allow-Methods": "GET, POST, OPTIONS",
      "Access-Control-Allow-Headers": "Content-Type, Authorization",
      "Access-Control-Allow-Credentials": "true",
      "Vary": "Origin"
    };

    const json = (data, status = 200, extraHeaders = {}) =>
      Response.json(data, {
        status,
        headers: {
          ...corsHeaders,
          ...extraHeaders
        }
      });

    if (request.method === "OPTIONS") {
      return new Response(null, {
        status: 204,
        headers: corsHeaders
      });
    }

    /*
     * ------------------------------------------------------------
     * Helpers
     * ------------------------------------------------------------
     */

    const encoder = new TextEncoder();

    function bytesToBase64Url(bytes) {
      let binary = "";
      for (const byte of bytes) {
        binary += String.fromCharCode(byte);
      }

      return btoa(binary)
        .replace(/\+/g, "-")
        .replace(/\//g, "_")
        .replace(/=+$/g, "");
    }

    function base64UrlToBytes(value) {
      const base64 = value
        .replace(/-/g, "+")
        .replace(/_/g, "/")
        .padEnd(Math.ceil(value.length / 4) * 4, "=");

      const binary = atob(base64);
      const bytes = new Uint8Array(binary.length);

      for (let i = 0; i < binary.length; i++) {
        bytes[i] = binary.charCodeAt(i);
      }

      return bytes;
    }

    function constantTimeEqual(a, b) {
      if (a.length !== b.length) {
        return false;
      }

      let result = 0;

      for (let i = 0; i < a.length; i++) {
        result |= a[i] ^ b[i];
      }

      return result === 0;
    }

    async function hashPassword(password, saltBytes) {
      const key = await crypto.subtle.importKey(
        "raw",
        encoder.encode(password),
        "PBKDF2",
        false,
        ["deriveBits"]
      );

      const bits = await crypto.subtle.deriveBits(
        {
          name: "PBKDF2",
          salt: saltBytes,
          iterations: 210000,
          hash: "SHA-256"
        },
        key,
        256
      );

      return new Uint8Array(bits);
    }

    async function createPasswordHash(password) {
      const salt = crypto.getRandomValues(new Uint8Array(32));
      const hash = await hashPassword(password, salt);

      return [
        "pbkdf2",
        "210000",
        bytesToBase64Url(salt),
        bytesToBase64Url(hash)
      ].join("$");
    }

    async function verifyPassword(password, stored) {
      try {
        const parts = stored.split("$");

        if (parts.length !== 4 || parts[0] !== "pbkdf2") {
          return false;
        }

        const iterations = Number(parts[1]);

        if (!Number.isInteger(iterations) || iterations < 100000) {
          return false;
        }

        const salt = base64UrlToBytes(parts[2]);
        const expectedHash = base64UrlToBytes(parts[3]);

        const key = await crypto.subtle.importKey(
          "raw",
          encoder.encode(password),
          "PBKDF2",
          false,
          ["deriveBits"]
        );

        const bits = await crypto.subtle.deriveBits(
          {
            name: "PBKDF2",
            salt,
            iterations,
            hash: "SHA-256"
          },
          key,
          256
        );

        return constantTimeEqual(
          new Uint8Array(bits),
          expectedHash
        );
      } catch {
        return false;
      }
    }

    function getSessionId(request) {
      const cookieHeader = request.headers.get("Cookie") || "";

      const cookies = cookieHeader.split(";");

      for (const cookie of cookies) {
        const [name, ...parts] = cookie.trim().split("=");

        if (name === "daemon_session") {
          return parts.join("=");
        }
      }

      return null;
    }

    function sessionCookie(sessionId) {
      return [
        `daemon_session=${sessionId}`,
        "Path=/",
        "HttpOnly",
        "Secure",
        "SameSite=Lax",
        "Max-Age=2592000"
      ].join("; ");
    }

    function clearSessionCookie() {
      return [
        "daemon_session=",
        "Path=/",
        "HttpOnly",
        "Secure",
        "SameSite=Lax",
        "Max-Age=0"
      ].join("; ");
    }

    async function getAuthenticatedUser(request) {
      const sessionId = getSessionId(request);

      if (!sessionId) {
        return null;
      }

      const result = await env.DB.prepare(
        `SELECT
          u.id,
          u.email,
          u.name,
          u.plan,
          u.status,
          u.stripe_customer_id,
          u.created_at,
          u.updated_at,
          s.id AS session_id,
          s.expires_at
         FROM sessions s
         JOIN users u ON u.id = s.user_id
         WHERE s.id = ?
         LIMIT 1`
      )
        .bind(sessionId)
        .first();

      if (!result) {
        return null;
      }

      if (new Date(result.expires_at).getTime() <= Date.now()) {
        await env.DB.prepare(
          "DELETE FROM sessions WHERE id = ?"
        )
          .bind(sessionId)
          .run();

        return null;
      }

      if (result.status !== "active") {
        return null;
      }

      return result;
    }

    async function requireUser(request) {
      const user = await getAuthenticatedUser(request);

      if (!user) {
        return {
          user: null,
          response: json(
            { error: "Unauthorized" },
            401
          )
        };
      }

      return {
        user,
        response: null
      };
    }

    /*
     * ------------------------------------------------------------
     * Health
     * ------------------------------------------------------------
     */

    if (url.pathname === "/api/health" && request.method === "GET") {
      return json({
        ok: true,
        service: "daemon-os",
        status: "operational",
        database: "connected",
        timestamp: new Date().toISOString()
      });
    }

    /*
     * ------------------------------------------------------------
     * AUTH — REGISTER
     * ------------------------------------------------------------
     */

    if (
      url.pathname === "/api/auth/register" &&
      request.method === "POST"
    ) {
      let body;

      try {
        body = await request.json();
      } catch {
        return json({ error: "Invalid JSON" }, 400);
      }

      const email = String(body.email || "")
        .trim()
        .toLowerCase();

      const password = String(body.password || "");
      const name = String(body.name || "").trim();

      if (!email || !email.includes("@") || email.length > 254) {
        return json(
          { error: "A valid email address is required" },
          400
        );
      }

      if (password.length < 8) {
        return json(
          { error: "Password must be at least 8 characters" },
          400
        );
      }

      if (password.length > 128) {
        return json(
          { error: "Password is too long" },
          400
        );
      }

      const existing = await env.DB.prepare(
        "SELECT id FROM users WHERE email = ? LIMIT 1"
      )
        .bind(email)
        .first();

      if (existing) {
        return json(
          { error: "An account with that email already exists" },
          409
        );
      }

      const userId = crypto.randomUUID();
      const sessionId = crypto.randomUUID();
      const now = new Date();
      const expiresAt = new Date(
        now.getTime() + 30 * 24 * 60 * 60 * 1000
      );

      let passwordHash;
      try {
        passwordHash = await createPasswordHash(password);
      } catch (error) {
        return json({ error: "Password hashing failed", detail: String(error?.message || error) }, 500);
      }

      await env.DB.prepare(
        `INSERT INTO users
          (id, email, password_hash, name, plan, status, created_at, updated_at)
         VALUES (?, ?, ?, ?, 'personal', 'active', ?, ?)`
      )
        .bind(
          userId,
          email,
          passwordHash,
          name || null,
          now.toISOString(),
          now.toISOString()
        )
        .run();

      await env.DB.prepare(
        `INSERT INTO sessions
          (id, user_id, expires_at, created_at)
         VALUES (?, ?, ?, ?)`
      )
        .bind(
          sessionId,
          userId,
          expiresAt.toISOString(),
          now.toISOString()
        )
        .run();

      return json(
        {
          ok: true,
          user: {
            id: userId,
            email,
            name: name || null,
            plan: "personal",
            status: "active"
          }
        },
        201,
        {
          "Set-Cookie": sessionCookie(sessionId)
        }
      );
    }

    /*
     * ------------------------------------------------------------
     * AUTH — LOGIN
     * ------------------------------------------------------------
     */

    if (
      url.pathname === "/api/auth/login" &&
      request.method === "POST"
    ) {
      let body;

      try {
        body = await request.json();
      } catch {
        return json({ error: "Invalid JSON" }, 400);
      }

      const email = String(body.email || "")
        .trim()
        .toLowerCase();

      const password = String(body.password || "");

      if (!email || !password) {
        return json(
          { error: "Email and password are required" },
          400
        );
      }

      const user = await env.DB.prepare(
        `SELECT
          id,
          email,
          password_hash,
          name,
          plan,
          status
         FROM users
         WHERE email = ?
         LIMIT 1`
      )
        .bind(email)
        .first();

      if (!user) {
        return json(
          { error: "Invalid email or password" },
          401
        );
      }

      const validPassword = await verifyPassword(
        password,
        user.password_hash
      );

      if (!validPassword || user.status !== "active") {
        return json(
          { error: "Invalid email or password" },
          401
        );
      }

      const sessionId = crypto.randomUUID();
      const now = new Date();
      const expiresAt = new Date(
        now.getTime() + 30 * 24 * 60 * 60 * 1000
      );

      await env.DB.prepare(
        `INSERT INTO sessions
          (id, user_id, expires_at, created_at)
         VALUES (?, ?, ?, ?)`
      )
        .bind(
          sessionId,
          user.id,
          expiresAt.toISOString(),
          now.toISOString()
        )
        .run();

      return json(
        {
          ok: true,
          user: {
            id: user.id,
            email: user.email,
            name: user.name,
            plan: user.plan,
            status: user.status
          }
        },
        200,
        {
          "Set-Cookie": sessionCookie(sessionId)
        }
      );
    }

    /*
     * ------------------------------------------------------------
     * AUTH — LOGOUT
     * ------------------------------------------------------------
     */

    if (
      url.pathname === "/api/auth/logout" &&
      request.method === "POST"
    ) {
      const sessionId = getSessionId(request);

      if (sessionId) {
        await env.DB.prepare(
          "DELETE FROM sessions WHERE id = ?"
        )
          .bind(sessionId)
          .run();
      }

      return json(
        {
          ok: true,
          logged_out: true
        },
        200,
        {
          "Set-Cookie": clearSessionCookie()
        }
      );
    }

    /*
     * ------------------------------------------------------------
     * AUTH — CURRENT USER
     * ------------------------------------------------------------
     */

    if (
      url.pathname === "/api/auth/me" &&
      request.method === "GET"
    ) {
      const { user, response } = await requireUser(request);

      if (response) {
        return response;
      }

      return json({
        ok: true,
        user: {
          id: user.id,
          email: user.email,
          name: user.name,
          plan: user.plan,
          status: user.status,
          stripe_customer_id: user.stripe_customer_id,
          created_at: user.created_at
        }
      });
    }

    /*
     * ------------------------------------------------------------
     * DEVICES — LIST
     * ------------------------------------------------------------
     */

    if (
      url.pathname === "/api/devices" &&
      request.method === "GET"
    ) {
      const { user, response } = await requireUser(request);

      if (response) {
        return response;
      }

      const result = await env.DB.prepare(
        `SELECT
          id,
          device_name,
          platform,
          status,
          last_seen_at,
          created_at
         FROM devices
         WHERE user_id = ?
         ORDER BY created_at DESC`
      )
        .bind(user.id)
        .all();

      return json({
        ok: true,
        devices: result.results || []
      });
    }

    /*
     * ------------------------------------------------------------
     * DEVICES — REGISTER
     * ------------------------------------------------------------
     */

    if (
      url.pathname === "/api/devices" &&
      request.method === "POST"
    ) {
      const { user, response } = await requireUser(request);

      if (response) {
        return response;
      }

      let body;

      try {
        body = await request.json();
      } catch {
        return json({ error: "Invalid JSON" }, 400);
      }

      const deviceId = String(
        body.id || crypto.randomUUID()
      ).trim();

      const deviceName = String(
        body.device_name || "Daemon Device"
      ).trim();

      const platform = String(
        body.platform || "unknown"
      ).trim();

      if (!deviceId || deviceId.length > 200) {
        return json(
          { error: "Invalid device ID" },
          400
        );
      }

      const now = new Date().toISOString();

      try {
        await env.DB.prepare(
          `INSERT INTO devices
            (id, user_id, device_name, platform, status, last_seen_at, created_at)
           VALUES (?, ?, ?, ?, 'active', ?, ?)`
        )
          .bind(
            deviceId,
            user.id,
            deviceName || "Daemon Device",
            platform || "unknown",
            now,
            now
          )
          .run();
      } catch {
        return json(
          { error: "Device already exists or could not be registered" },
          409
        );
      }

      return json(
        {
          ok: true,
          device: {
            id: deviceId,
            device_name: deviceName || "Daemon Device",
            platform: platform || "unknown",
            status: "active",
            last_seen_at: now,
            created_at: now
          }
        },
        201
      );
    }

    /*
     * ------------------------------------------------------------
     * SUBSCRIPTION
     * ------------------------------------------------------------
     */

    if (
      url.pathname === "/api/subscription" &&
      request.method === "GET"
    ) {
      const { user, response } = await requireUser(request);

      if (response) {
        return response;
      }

      const subscription = await env.DB.prepare(
        `SELECT
          id,
          stripe_customer_id,
          stripe_subscription_id,
          plan,
          status,
          current_period_end,
          created_at,
          updated_at
         FROM subscriptions
         WHERE user_id = ?
         ORDER BY updated_at DESC
         LIMIT 1`
      )
        .bind(user.id)
        .first();

      return json({
        ok: true,
        plan: user.plan,
        subscription: subscription || null
      });
    }

    /*
     * ------------------------------------------------------------
     * AUTHENTICATED TELEMETRY HEARTBEAT
     *
     * Existing ingestion path preserved for now.
     * Tenant/device-scoped telemetry hardening comes next.
     * ------------------------------------------------------------
     */

    if (
      url.pathname === "/api/heartbeat" &&
      request.method === "POST"
    ) {
      const auth = request.headers.get("Authorization");

      if (!auth || !auth.startsWith("Bearer ")) {
        return json(
          { error: "Unauthorized" },
          401
        );
      }

      const token = auth.slice(7).trim();
      const configuredToken =
        (env.DAEMON_INGEST_TOKEN || "").trim();

      if (!configuredToken || token !== configuredToken) {
        return json(
          { error: "Unauthorized" },
          401
        );
      }

      let body;

      try {
        body = await request.json();
      } catch {
        return json(
          { error: "Invalid JSON" },
          400
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

      return json({
        ok: true,
        received: true,
        timestamp: receivedAt,
        device_id: deviceId
      });
    }

    /*
     * ------------------------------------------------------------
     * FRONTEND
     * ------------------------------------------------------------
     */

    return env.ASSETS.fetch(request);
  }
};
