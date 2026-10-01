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
    const securityHeaders = {
      "Strict-Transport-Security": "max-age=31536000; includeSubDomains",
      "Content-Security-Policy": "default-src 'self'; script-src 'self' 'unsafe-inline'; style-src 'self' 'unsafe-inline'; img-src 'self' data: https:; connect-src 'self' https:; font-src 'self' data: https:; frame-src 'self' https://checkout.stripe.com https://billing.stripe.com; frame-ancestors 'none'; base-uri 'self'; form-action 'self' https://checkout.stripe.com https://billing.stripe.com; object-src 'none'",
      "X-Content-Type-Options": "nosniff",
      "Referrer-Policy": "strict-origin-when-cross-origin",
      "Permissions-Policy": "camera=(), microphone=(), geolocation=(), payment=(self)",
      "X-Frame-Options": "DENY"
    };


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
        headers: { ...securityHeaders, ...corsHeaders, ...extraHeaders }
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

    /*
     * ------------------------------------------------------------
     * STRIPE BILLING HELPERS
     * ------------------------------------------------------------
     */

    const STRIPE_PRICES = {
      personal: "price_1UBX32H0QiF8TJHDbUmGzREo",
      pro: "price_1UBBV0H0QiF8TJHDO2UL3JQN",
      business: "price_1UBUAbH0QiF8TJHDXVQk54XF"
    };

    const STRIPE_PLANS_BY_PRICE = {
      "price_1UBX32H0QiF8TJHDbUmGzREo": "personal",
      "price_1UBBV0H0QiF8TJHDO2UL3JQN": "pro",
      "price_1UBUAbH0QiF8TJHDXVQk54XF": "business"
    };

    function stripeAuthHeader(secret) {
      return "Basic " + btoa(`${secret}:`);
    }

    async function stripeRequest(path, options = {}) {
      const secret = (env.STRIPE_SECRET_KEY || "").trim();

      if (!secret) {
        throw new Error("STRIPE_SECRET_KEY is not configured");
      }

      const response = await fetch(`https://api.stripe.com${path}`, {
        ...options,
        headers: {
          Authorization: stripeAuthHeader(secret),
          ...(options.headers || {})
        }
      });

      const text = await response.text();

      let data;

      try {
        data = JSON.parse(text);
      } catch {
        data = { raw: text };
      }

      if (!response.ok) {
        throw new Error(
          data?.error?.message ||
          `Stripe API request failed with HTTP ${response.status}`
        );
      }

      return data;
    }

    function toIsoFromUnixSeconds(value) {
      if (!value) {
        return null;
      }

      return new Date(Number(value) * 1000).toISOString();
    }

    function getPlanFromSubscription(subscription) {
      const priceId =
        subscription?.items?.data?.[0]?.price?.id ||
        subscription?.plan?.id ||
        null;

      return STRIPE_PLANS_BY_PRICE[priceId] || null;
    }

    async function verifyStripeSignature(payload, signatureHeader) {
      const secret = (env.STRIPE_WEBHOOK_SECRET || "").trim();

      if (!secret || !signatureHeader) {
        return false;
      }

      const parts = signatureHeader.split(",");
      const timestampPart = parts.find((part) =>
        part.startsWith("t=")
      );

      if (!timestampPart) {
        return false;
      }

      const timestamp = Number(timestampPart.slice(2));

      if (!Number.isFinite(timestamp)) {
        return false;
      }

      const age = Math.abs(
        Math.floor(Date.now() / 1000) - timestamp
      );

      if (age > 300) {
        return false;
      }

      const signedPayload = `${timestamp}.${payload}`;

      const key = await crypto.subtle.importKey(
        "raw",
        encoder.encode(secret),
        {
          name: "HMAC",
          hash: "SHA-256"
        },
        false,
        ["sign"]
      );

      const signature = new Uint8Array(
        await crypto.subtle.sign(
          "HMAC",
          key,
          encoder.encode(signedPayload)
        )
      );

      let expected = "";

      for (const byte of signature) {
        expected += byte.toString(16).padStart(2, "0");
      }

      const suppliedSignatures = parts
        .filter((part) => part.startsWith("v1="))
        .map((part) => part.slice(3));

      return suppliedSignatures.some((supplied) =>
        constantTimeEqual(
          encoder.encode(expected),
          encoder.encode(supplied)
        )
      );
    }

    async function syncStripeSubscription(subscription) {
      const customerId = subscription?.customer;
      const subscriptionId = subscription?.id;

      if (!customerId || !subscriptionId) {
        return;
      }

      const priceId =
        subscription?.items?.data?.[0]?.price?.id || null;

      const plan =
        STRIPE_PLANS_BY_PRICE[priceId] ||
        subscription?.metadata?.plan ||
        "personal";

      let userId =
        subscription?.metadata?.user_id ||
        null;

      if (!userId) {
        const user = await env.DB.prepare(
          `SELECT id
           FROM users
           WHERE stripe_customer_id = ?
           LIMIT 1`
        )
          .bind(customerId)
          .first();

        userId = user?.id || null;
      }

      if (!userId) {
        return;
      }

      const now = new Date().toISOString();
      const currentPeriodEnd =
        toIsoFromUnixSeconds(subscription.current_period_end);

      await env.DB.prepare(
        `INSERT INTO subscriptions (
          id,
          user_id,
          stripe_customer_id,
          stripe_subscription_id,
          plan,
          status,
          current_period_end,
          created_at,
          updated_at
        )
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
        ON CONFLICT(stripe_subscription_id)
        DO UPDATE SET
          stripe_customer_id = excluded.stripe_customer_id,
          plan = excluded.plan,
          status = excluded.status,
          current_period_end = excluded.current_period_end,
          updated_at = excluded.updated_at`
      )
        .bind(
          crypto.randomUUID(),
          userId,
          customerId,
          subscriptionId,
          plan,
          subscription.status || "unknown",
          currentPeriodEnd,
          now,
          now
        )
        .run();

      const accessActive = [
        "trialing",
        "active"
      ].includes(subscription.status);

      await env.DB.prepare(
        `UPDATE users
         SET
           plan = ?,
           status = ?,
           stripe_customer_id = ?,
           updated_at = ?
         WHERE id = ?`
      )
        .bind(
          plan,
          accessActive ? "active" : subscription.status || "inactive",
          customerId,
          now,
          userId
        )
        .run();
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
          iterations: 100000,
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
        "100000",
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

      const passwordHash = await createPasswordHash(password);

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
     * STRIPE CHECKOUT
     * ------------------------------------------------------------
     */

    if (
      url.pathname === "/api/billing/checkout" &&
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

      const requestedPlan =
        String(body?.plan || "personal").toLowerCase();

      const priceId = STRIPE_PRICES[requestedPlan];

      if (!priceId) {
        return json(
          {
            error: "Invalid plan",
            allowed_plans: ["personal", "pro", "business"]
          },
          400
        );
      }

      try {
        let customerId = user.stripe_customer_id || null;

        if (!customerId) {
          const customerParams = new URLSearchParams();

          customerParams.set("email", user.email);
          customerParams.set(
            "name",
            user.name || user.email
          );
          customerParams.set(
            "metadata[user_id]",
            user.id
          );

          const customer = await stripeRequest(
            "/v1/customers",
            {
              method: "POST",
              headers: {
                "Content-Type":
                  "application/x-www-form-urlencoded"
              },
              body: customerParams.toString()
            }
          );

          customerId = customer.id;

          await env.DB.prepare(
            `UPDATE users
             SET stripe_customer_id = ?, updated_at = ?
             WHERE id = ?`
          )
            .bind(
              customerId,
              new Date().toISOString(),
              user.id
            )
            .run();
        }

        const checkoutParams = new URLSearchParams();

        checkoutParams.set(
          "mode",
          "subscription"
        );

        checkoutParams.set(
          "customer",
          customerId
        );

        checkoutParams.set(
          "line_items[0][price]",
          priceId
        );

        checkoutParams.set(
          "line_items[0][quantity]",
          "1"
        );

        checkoutParams.set(
          "subscription_data[trial_period_days]",
          "14"
        );

        checkoutParams.set(
          "subscription_data[metadata][user_id]",
          user.id
        );

        checkoutParams.set(
          "subscription_data[metadata][plan]",
          requestedPlan
        );

        checkoutParams.set(
          "metadata[user_id]",
          user.id
        );

        checkoutParams.set(
          "metadata[plan]",
          requestedPlan
        );

        checkoutParams.set(
          "success_url",
          `${url.origin}/?billing=success&session_id={CHECKOUT_SESSION_ID}`
        );

        checkoutParams.set(
          "cancel_url",
          `${url.origin}/?billing=cancelled`
        );

        const session = await stripeRequest(
          "/v1/checkout/sessions",
          {
            method: "POST",
            headers: {
              "Content-Type":
                "application/x-www-form-urlencoded"
            },
            body: checkoutParams.toString()
          }
        );

        return json({
          ok: true,
          plan: requestedPlan,
          checkout_url: session.url,
          session_id: session.id
        });
      } catch (error) {
        return json(
          {
            error: "Unable to create checkout session",
            message: error.message
          },
          502
        );
      }
    }

    /*
     * ------------------------------------------------------------
     * STRIPE WEBHOOK
     * ------------------------------------------------------------
     */

    if (
      url.pathname === "/api/stripe/webhook" &&
      request.method === "POST"
    ) {
      const payload = await request.text();

      const signature =
        request.headers.get("Stripe-Signature");

      const valid =
        await verifyStripeSignature(
          payload,
          signature
        );

      if (!valid) {
        return json(
          { error: "Invalid Stripe signature" },
          400
        );
      }

      let event;

      try {
        event = JSON.parse(payload);
      } catch {
        return json(
          { error: "Invalid webhook JSON" },
          400
        );
      }

      try {
        switch (event.type) {
          case "checkout.session.completed": {
            const session = event.data.object;

            const userId =
              session.metadata?.user_id || null;

            const customerId =
              session.customer || null;

            if (userId && customerId) {
              await env.DB.prepare(
                `UPDATE users
                 SET stripe_customer_id = ?, updated_at = ?
                 WHERE id = ?`
              )
                .bind(
                  customerId,
                  new Date().toISOString(),
                  userId
                )
                .run();
            }

            break;
          }

          case "customer.subscription.created":
          case "customer.subscription.updated":
          case "customer.subscription.deleted": {
            await syncStripeSubscription(
              event.data.object
            );

            break;
          }

          case "invoice.paid": {
            const invoice = event.data.object;

            if (invoice.subscription) {
              const subscription =
                await stripeRequest(
                  `/v1/subscriptions/${invoice.subscription}`
                );

              await syncStripeSubscription(
                subscription
              );
            }

            break;
          }

          case "invoice.payment_failed": {
            const invoice = event.data.object;

            if (invoice.subscription) {
              const subscription =
                await stripeRequest(
                  `/v1/subscriptions/${invoice.subscription}`
                );

              await syncStripeSubscription(
                subscription
              );
            }

            break;
          }

          default:
            break;
        }

        return json({
          received: true
        });
      } catch (error) {
        return json(
          {
            error: "Webhook processing failed",
            message: error.message
          },
          500
        );
      }
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
     * STRIPE CUSTOMER PORTAL
     * ------------------------------------------------------------
     */

    if (
      url.pathname === "/api/billing/portal" &&
      request.method === "POST"
    ) {
      const { user, response } = await requireUser(request);

      if (response) {
        return response;
      }

      if (!user.stripe_customer_id) {
        return json(
          {
            error: "No Stripe customer",
            message: "This account does not have a Stripe billing profile yet."
          },
          400
        );
      }

      try {
        const portal = await stripeRequest(
          "/v1/billing_portal/sessions",
          {
            method: "POST",
            headers: {
              "Content-Type": "application/x-www-form-urlencoded"
            },
            body: new URLSearchParams({
              customer: user.stripe_customer_id,
              return_url: `${url.origin}/?billing=return`
            }).toString()
          }
        );

        return json({
          ok: true,
          portal_url: portal.url
        });
      } catch (error) {
        return json(
          {
            error: "Unable to create billing portal session",
            message: error.message
          },
          502
        );
      }
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

      if (deviceId === "daemon-live-test") {
        await env.TELEMETRY.put(
          "public:demo",
          JSON.stringify({
            ...body,
            received_at: receivedAt
          })
        );
      }

      return json({
        ok: true,
        received: true,
        timestamp: receivedAt,
        device_id: deviceId
      });
    }

    /*
     * ------------------------------------------------------------
     * PUBLIC TELEMETRY
     * ------------------------------------------------------------
     */

    if (
      url.pathname === "/telemetry" &&
      request.method === "GET"
    ) {
      const publicValue = await env.TELEMETRY.get("public:demo");
      const list = { keys: publicValue ? [{ name: "public:demo" }] : [] };

      if (!list.keys.length) {
        return json({
          cpu_usage: 0,
          memory_used_percent: 0,
          disk_used_percent: 0,
          status: "offline"
        });
      }

      const records = [];

      for (const key of list.keys) {
        const value = await env.TELEMETRY.get(key.name);
        if (!value) continue;

        try {
          records.push(JSON.parse(value));
        } catch {
          // Ignore malformed telemetry records.
        }
      }

      if (!records.length) {
        return json({
          cpu_usage: 0,
          memory_used_percent: 0,
          disk_used_percent: 0,
          status: "offline"
        });
      }

      records.sort((a, b) =>
        String(b.received_at || "").localeCompare(
          String(a.received_at || "")
        )
      );

      const record = records[0];
      const telemetry = record.telemetry || record;

      const memoryTotal = Number(telemetry.memory_total || 0);
      const diskTotal = Number(telemetry.disk_total || 0);
      const memoryUsed = Number(telemetry.memory_used || 0);
      const diskUsed = Number(telemetry.disk_used || 0);
      const memoryPercent = memoryTotal ? (memoryUsed / memoryTotal) * 100 : memoryUsed;
      const diskPercent = diskTotal ? (diskUsed / diskTotal) * 100 : diskUsed;

      return json({
        cpu_usage: Number(telemetry.cpu_usage || 0),
        memory_used_percent: memoryPercent,
        disk_used_percent: diskPercent,
        memory_used: memoryUsed,
        memory_total: memoryTotal,
        disk_used: diskUsed,
        disk_total: diskTotal,
        uptime: Number(telemetry.uptime || 0),
        received_at: record.received_at || null,
        device_id: record.device_id || null,
        status: "online"
      });
    }
    /*
     * ------------------------------------------------------------
     * PUBLIC HISTORY
     * ------------------------------------------------------------
     */

    if (
      url.pathname === "/history" &&
      request.method === "GET"
    ) {
      const publicValue = await env.TELEMETRY.get("public:demo");
      const list = { keys: publicValue ? [{ name: "public:demo" }] : [] };
      const history = [];

      for (const key of list.keys) {
        const value = await env.TELEMETRY.get(key.name);
        if (!value) continue;

        try {
          const record = JSON.parse(value);
          const telemetry = record.telemetry || record;

          history.push({
            timestamp: Math.floor(
              new Date(record.received_at || 0).getTime() / 1000
            ),
            telemetry: {
              cpu_usage: Number(telemetry.cpu_usage || 0),
              memory_used: Number(telemetry.memory_used || 0),
              memory_total: Number(telemetry.memory_total || 0),
              disk_used: Number(telemetry.disk_used || 0),
              disk_total: Number(telemetry.disk_total || 0),
              uptime: Number(telemetry.uptime || 0)
            }
          });
        } catch {
          // Ignore malformed telemetry records.
        }
      }

      history.sort((a, b) => b.timestamp - a.timestamp);

      return json(history.slice(0, 50));
    }
        /*
     * ------------------------------------------------------------
     * PUBLIC EVENTS
     * ------------------------------------------------------------
     */

    if (url.pathname === "/events" && request.method === "GET") {
      const publicValue = await env.TELEMETRY.get("public:demo");
      const list = { keys: publicValue ? [{ name: "public:demo" }] : [] };
      const events = [];

      for (const key of list.keys) {
        const value = await env.TELEMETRY.get(key.name);
        if (!value) continue;

        try {
          const record = JSON.parse(value);
          const telemetry = record.telemetry || record;

          const cpu = Number(telemetry.cpu_usage || 0);
          const memoryTotal = Number(telemetry.memory_total || 0);
          const memoryUsed = Number(telemetry.memory_used || 0);
          const diskTotal = Number(telemetry.disk_total || 0);
          const diskUsed = Number(telemetry.disk_used || 0);

          const memoryPercent = memoryTotal
            ? (memoryUsed / memoryTotal) * 100
            : 0;

          const diskPercent = diskTotal
            ? (diskUsed / diskTotal) * 100
            : 0;

          if (cpu >= 75 || memoryPercent >= 80 || diskPercent >= 85) {
            const signals = [];

            if (cpu >= 75) signals.push("CPU utilization elevated.");
            if (memoryPercent >= 80) signals.push("Memory utilization elevated.");
            if (diskPercent >= 85) signals.push("Disk utilization elevated.");

            events.push({
              id:       "telemetry-" + Date.now(),
              name: "Resource Threshold",
              description: signals.join(" "),
              status:
                cpu >= 90 || memoryPercent >= 90 || diskPercent >= 95
                  ? "CRITICAL"
                  : "WARNING",
              updated_at: Math.floor(
                new Date(record.received_at || 0).getTime() / 1000
              ),
              evidence: signals,
              confidence: 0.95
            });
          }
        } catch {}
      }

      return json(events.slice(0, 50));
    }
    /*
     * ------------------------------------------------------------
     * PUBLIC SECURITY
     * ------------------------------------------------------------
     */

    if (url.pathname === "/security" && request.method === "GET") {
      return json({
        security_score: null,
        processes: null,
        high_cpu_processes: null,
        high_memory_processes: null,
        status: "MONITORING",
        message: "Security monitoring is available when a Daemon device is connected."
      });
    }
    /*
     * ------------------------------------------------------------
     * DAEMON UNIFIED INTELLIGENCE ENGINE
     * ------------------------------------------------------------
     */

    if (url.pathname === "/intelligence" && request.method === "GET") {
      const cacheKey = "cache:intelligence";
      const cached = await env.TELEMETRY.get(cacheKey);

      let previous = null;

      if (cached) {
        try {
          previous = JSON.parse(cached);
        } catch {}
      }

      const publicValue = await env.TELEMETRY.get("public:demo");

      if (!publicValue) {
        const offline = {
          status: "OFFLINE",
          health_score: 0,
          intelligence_state: "NO_SIGNAL",
          observability: {
            infrastructure: "OFFLINE",
            ai_systems: "OFFLINE",
            telemetry: "OFFLINE"
          },
          signals: [],
          anomalies: [],
          changes: [],
          correlations: [],
          impact: [],
          risk: [],
          root_cause_hypotheses: [],
          recommendations: [
            "Connect a Daemon device to begin unified infrastructure intelligence."
          ],
          next_actions: [
            "Connect telemetry",
            "Establish a baseline",
            "Begin dependency and workload correlation"
          ],
          trend: "STABLE",
          anomaly_detected: false,
          received_at: null,
          device_id: null
        };

        await env.TELEMETRY.put(
          cacheKey,
          JSON.stringify(offline),
          { expirationTtl: 60 }
        );

        return json(offline);
      }

      let record;

      try {
        record = JSON.parse(publicValue);
      } catch {
        record = null;
      }

      if (!record) {
        return json({
          status: "DEGRADED",
          health_score: 0,
          intelligence_state: "INVALID_TELEMETRY",
          signals: ["Telemetry payload could not be parsed."],
          anomalies: [],
          correlations: [],
          impact: [],
          risk: [],
          root_cause_hypotheses: [],
          recommendations: ["Inspect the connected telemetry source."]
        }, 503);
      }

      const telemetry = record.telemetry || record;

      const num = (value, fallback = 0) => {
        const n = Number(value);
        return Number.isFinite(n) ? n : fallback;
      };

      const pct = (used, total, direct) => {
        if (num(total) > 0) return (num(used) / num(total)) * 100;
        return num(direct);
      };

      const cpu = num(telemetry.cpu_usage ?? telemetry.cpu);
      const memoryPercent = pct(
        telemetry.memory_used,
        telemetry.memory_total,
        telemetry.memory_percent ?? telemetry.memory
      );
      const diskPercent = pct(
        telemetry.disk_used,
        telemetry.disk_total,
        telemetry.disk_percent ?? telemetry.disk
      );

      const gpuPercent = num(
        telemetry.gpu_usage ??
        telemetry.gpu_utilization ??
        telemetry.gpu_percent
      );

      const networkPercent = num(
        telemetry.network_utilization ??
        telemetry.network_percent
      );

      const inferenceLatency = num(
        telemetry.inference_latency_ms ??
        telemetry.ai?.inference_latency_ms
      );

      const tokenRate = num(
        telemetry.tokens_per_second ??
        telemetry.ai?.tokens_per_second
      );

      const errorRate = num(
        telemetry.error_rate ??
        telemetry.ai?.error_rate
      );

      const requestRate = num(
        telemetry.request_rate ??
        telemetry.ai?.request_rate
      );

      const agentFailures = num(
        telemetry.agent_failures ??
        telemetry.ai?.agent_failures
      );

      const ragLatency = num(
        telemetry.rag_latency_ms ??
        telemetry.ai?.rag_latency_ms
      );

      const vectorLatency = num(
        telemetry.vector_latency_ms ??
        telemetry.ai?.vector_latency_ms
      );

      const costRate = num(
        telemetry.cost_rate ??
        telemetry.cloud?.cost_rate
      );

      const services = telemetry.services || [];
      const workloads = telemetry.workloads || [];
      const dependencies = telemetry.dependencies || [];
      const aiSystems = telemetry.ai_systems || telemetry.ai?.systems || [];

      const cpuStatus =
        cpu >= 90 ? "CRITICAL" :
        cpu >= 75 ? "WARNING" :
        "HEALTHY";

      const memoryStatus =
        memoryPercent >= 90 ? "CRITICAL" :
        memoryPercent >= 80 ? "WARNING" :
        "HEALTHY";

      const diskStatus =
        diskPercent >= 95 ? "CRITICAL" :
        diskPercent >= 85 ? "WARNING" :
        "HEALTHY";

      const gpuStatus =
        gpuPercent >= 95 ? "CRITICAL" :
        gpuPercent >= 85 ? "WARNING" :
        "HEALTHY";

      const anomalySignals = [];
      const signals = [];
      const changes = [];
      const correlations = [];
      const impact = [];
      const risk = [];
      const recommendations = [];
      const rootCauseHypotheses = [];
      const nextActions = [];

      if (cpuStatus !== "HEALTHY") {
        signals.push(`CPU utilization ${cpu.toFixed(1)}%`);
      }

      if (memoryStatus !== "HEALTHY") {
        signals.push(`Memory utilization ${memoryPercent.toFixed(1)}%`);
      }

      if (diskStatus !== "HEALTHY") {
        signals.push(`Disk utilization ${diskPercent.toFixed(1)}%`);
      }

      if (gpuStatus !== "HEALTHY") {
        signals.push(`GPU utilization ${gpuPercent.toFixed(1)}%`);
      }

      if (errorRate > 5) {
        anomalySignals.push(`AI/application error rate is ${errorRate.toFixed(1)}%`);
      }

      if (agentFailures > 0) {
        anomalySignals.push(`${agentFailures} agent failures reported`);
      }

      if (inferenceLatency > 2000) {
        anomalySignals.push(`Inference latency is ${inferenceLatency.toFixed(0)}ms`);
      }

      if (ragLatency > 1000) {
        anomalySignals.push(`RAG latency is ${ragLatency.toFixed(0)}ms`);
      }

      if (vectorLatency > 1000) {
        anomalySignals.push(`Vector retrieval latency is ${vectorLatency.toFixed(0)}ms`);
      }

      if (previous) {
        const oldCpu = num(previous.metrics?.cpu_usage, num(previous.cpu_usage));
        const oldMemory = num(previous.metrics?.memory_percent, num(previous.memory_used_percent));
        const oldDisk = num(previous.metrics?.disk_percent, num(previous.disk_used_percent));

        const cpuChange = oldCpu ? ((cpu - oldCpu) / oldCpu) * 100 : 0;
        const memoryChange = oldMemory ? ((memoryPercent - oldMemory) / oldMemory) * 100 : 0;
        const diskChange = oldDisk ? ((diskPercent - oldDisk) / oldDisk) * 100 : 0;

        if (Math.abs(cpuChange) >= 20) {
          changes.push(`CPU changed ${cpuChange.toFixed(1)}% from the previous intelligence baseline.`);
        }

        if (Math.abs(memoryChange) >= 15) {
          changes.push(`Memory changed ${memoryChange.toFixed(1)}% from the previous intelligence baseline.`);
        }

        if (Math.abs(diskChange) >= 10) {
          changes.push(`Disk utilization changed ${diskChange.toFixed(1)}% from the previous intelligence baseline.`);
        }
      }

      if (cpu >= 75 && memoryPercent >= 80) {
        correlations.push(
          "CPU and memory pressure are occurring together, indicating possible workload saturation."
        );
        impact.push("Compute-bound workloads may experience latency or scheduling pressure.");
        rootCauseHypotheses.push(
          "A workload or deployment may be consuming disproportionate compute and memory resources."
        );
      }

      if (gpuPercent >= 85 && inferenceLatency > 1000) {
        correlations.push(
          "GPU utilization and inference latency are elevated together."
        );
        impact.push("AI inference workloads may be experiencing compute saturation.");
        rootCauseHypotheses.push(
          "GPU saturation may be contributing to increased model inference latency."
        );
      }

      if (ragLatency > 1000 && inferenceLatency > 1000) {
        correlations.push(
          "RAG retrieval latency and model inference latency are elevated together."
        );
        impact.push("AI response latency may be affected by both retrieval and inference stages.");
        rootCauseHypotheses.push(
          "Retrieval or vector-store latency may be propagating into end-to-end AI response latency."
        );
      }

      if (errorRate > 5 && agentFailures > 0) {
        correlations.push(
          "Application/AI errors and agent failures are occurring together."
        );
        impact.push("Agent workflows may be failing before completing their intended actions.");
        rootCauseHypotheses.push(
          "A shared dependency, tool, model, or downstream service may be contributing to agent failures."
        );
      }

      if (networkPercent >= 85 && requestRate > 0) {
        correlations.push(
          "High network utilization coincides with active request traffic."
        );
        impact.push("Network saturation may affect service and AI workload latency.");
      }

      if (diskPercent >= 90) {
        risk.push("Storage exhaustion risk");
        recommendations.push(
          "Investigate high-volume logs, caches, artifacts, containers, and persistent data."
        );
        nextActions.push("Inspect storage consumers and reclaim or expand capacity.");
      }

      if (cpu >= 90) {
        risk.push("Compute saturation risk");
        recommendations.push(
          "Identify the workloads consuming the most CPU and investigate sustained saturation."
        );
        nextActions.push("Inspect top CPU-consuming workloads and recent deployments.");
      }

      if (memoryPercent >= 90) {
        risk.push("Memory exhaustion risk");
        recommendations.push(
          "Investigate memory-heavy workloads and possible memory pressure or leaks."
        );
        nextActions.push("Inspect memory consumers, limits, requests, and recent workload changes.");
      }

      if (gpuPercent >= 90) {
        risk.push("GPU saturation risk");
        recommendations.push(
          "Investigate GPU-bound inference and AI workloads for queueing or capacity pressure."
        );
        nextActions.push("Inspect GPU workloads, queues, model utilization, and inference latency.");
      }

      if (errorRate > 5) {
        risk.push("Application/AI reliability risk");
        recommendations.push(
          "Correlate elevated errors with deployments, dependencies, agents, models, and downstream services."
        );
        nextActions.push("Trace failing requests through their dependency chain.");
      }

      if (dependencies.length > 0) {
        correlations.push(
          `${dependencies.length} dependency relationship(s) are available for cross-system correlation.`
        );
      }

      if (services.length > 0 || workloads.length > 0) {
        impact.push(
          `${services.length + workloads.length} service/workload signal(s) are available for blast-radius analysis.`
        );
      }

      if (aiSystems.length > 0) {
        signals.push(`${aiSystems.length} AI system signal(s) detected.`);
      }

      if (costRate > 0 && (cpu >= 75 || gpuPercent >= 75)) {
        correlations.push(
          "Infrastructure utilization and reported cost-rate signals are available for cost/performance correlation."
        );
        nextActions.push("Compare resource utilization with workload cost and performance.");
      }

      if (!recommendations.length) {
        recommendations.push(
          "No immediate resource threshold breach detected. Continue baseline collection and correlation."
        );
      }

      if (!nextActions.length) {
        nextActions.push(
          "Continue collecting telemetry to strengthen historical baselines and dependency intelligence."
        );
      }

      let healthScore = 100;

      for (const statusValue of [
        cpuStatus,
        memoryStatus,
        diskStatus,
        gpuStatus
      ]) {
        if (statusValue === "CRITICAL") healthScore -= 22;
        else if (statusValue === "WARNING") healthScore -= 10;
      }

      if (errorRate > 5) healthScore -= 12;
      if (agentFailures > 0) healthScore -= 8;
      if (inferenceLatency > 2000) healthScore -= 6;
      if (risk.length >= 3) healthScore -= 8;

      healthScore = Math.max(0, Math.min(100, Math.round(healthScore)));

      const status =
        healthScore >= 90 ? "HEALTHY" :
        healthScore >= 70 ? "DEGRADED" :
        healthScore >= 40 ? "AT_RISK" :
        "CRITICAL";

      const anomalyDetected =
        anomalySignals.length > 0 ||
        changes.length > 0 ||
        correlations.length > 0;

      const intelligenceState =
        risk.length >= 3 ? "HIGH_RISK" :
        anomalyDetected ? "INVESTIGATING" :
        "BASELINE";

      const trend =
        changes.some(change => /changed -/i.test(change))
          ? "CHANGING"
          : changes.length
            ? "CHANGING"
            : "STABLE";

      const intelligence = {
        status,
        health_score: healthScore,
        intelligence_state: intelligenceState,

        observability: {
          infrastructure: "ONLINE",
          ai_systems: aiSystems.length || inferenceLatency || gpuPercent ? "ACTIVE" : "READY",
          telemetry: "ACTIVE"
        },

        metrics: {
          cpu_usage: cpu,
          memory_percent: memoryPercent,
          disk_percent: diskPercent,
          gpu_percent: gpuPercent,
          network_percent: networkPercent,
          inference_latency_ms: inferenceLatency,
          rag_latency_ms: ragLatency,
          vector_latency_ms: vectorLatency,
          request_rate: requestRate,
          error_rate: errorRate,
          agent_failures: agentFailures,
          tokens_per_second: tokenRate,
          cost_rate: costRate
        },

        resource_status: {
          cpu: cpuStatus,
          memory: memoryStatus,
          disk: diskStatus,
          gpu: gpuStatus
        },

        signals,
        anomalies: anomalySignals,
        changes,
        correlations,
        impact,
        risk,
        root_cause_hypotheses: rootCauseHypotheses,
        recommendations,
        next_actions: nextActions,

        intelligence_graph: {
          application: telemetry.application || telemetry.app || null,
          ai: telemetry.ai || null,
          data: telemetry.data || null,
          compute: telemetry.compute || null,
          cloud: telemetry.cloud || null,
          dependencies,
          services,
          workloads
        },

        trend,
        anomaly_detected: anomalyDetected,

        metrics_collected: {
          infrastructure: true,
          kubernetes: Boolean(
            telemetry.kubernetes ||
            telemetry.cluster ||
            telemetry.pods ||
            telemetry.nodes
          ),
          cloud: Boolean(telemetry.cloud),
          bare_metal: Boolean(telemetry.bare_metal),
          ai: Boolean(
            telemetry.ai ||
            telemetry.ai_systems ||
            inferenceLatency ||
            gpuPercent
          ),
          rag: Boolean(
            telemetry.rag ||
            ragLatency
          ),
          vector_database: Boolean(
            telemetry.vector_database ||
            vectorLatency
          ),
          inference: Boolean(
            telemetry.inference ||
            inferenceLatency
          )
        },

        received_at: record.received_at || new Date().toISOString(),
        device_id: record.device_id || null
      };

      await env.TELEMETRY.put(
        cacheKey,
        JSON.stringify(intelligence),
        { expirationTtl: 60 }
      );

      return json(intelligence);
    }

/*
     * ------------------------------------------------------------
     * FRONTEND
     * ------------------------------------------------------------
     */

        const assetResponse = await env.ASSETS.fetch(request);
    const securedResponse = new Response(assetResponse.body, assetResponse);

    for (const [name, value] of Object.entries(securityHeaders)) {
      securedResponse.headers.set(name, value);
    }

    return securedResponse;
  }
};

