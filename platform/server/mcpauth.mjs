// Вход нейросети в MCP по OAuth — без ключа в ссылке.
//
// Ключ в ссылке ломался двумя путями: его перевыпускали, и старая ссылка в
// нейросети молча переставала работать, а у человека с несколькими аккаунтами
// (почта, Google, Telegram) ссылка была своя у каждого. Claude и другие
// клиенты MCP умеют входить сами: получают 401 со ссылкой на описание сервера,
// регистрируются, открывают страницу входа на сайте и дальше продлевают
// доступ без человека. Адрес для всех один — `/mcp`.
//
// Выданный доступ — обычный ключ площадки в таблице tokens: его видно в
// кабинете, его можно отозвать, и `/mcp` проверяет его тем же userForKey.

import { createHash, randomBytes } from "node:crypto";

const CODE_TTL = 10 * 60_000;
const PENDING_TTL = 15 * 60_000;
/** Сколько живёт доступ: продлевается сам по refresh-токену. */
const ACCESS_DAYS = 30;

const sha = (text) => createHash("sha256").update(text).digest("hex");
/** Нейросети, чей адрес возврата узнаём: к ним предупреждения на согласии нет. */
const KNOWN_HOSTS = ["claude.ai", "claude.com", "anthropic.com", "chatgpt.com", "openai.com", "cursor.com", "cursor.sh", "localhost", "127.0.0.1"];
/** Регистраций клиентов с одного адреса за час — хватит любой нейросети. */
const REGISTER_PER_HOUR = 20;
const b64url = (buffer) => buffer.toString("base64url");
const escape = (text) => String(text ?? "").replace(/[&<>"']/g, (ch) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[ch]);

export function mountMcpAuth({ db, publicUrl, sessionUser }) {
  db.exec(`
    CREATE TABLE IF NOT EXISTS mcp_clients (
      id TEXT PRIMARY KEY,
      name TEXT NOT NULL,
      redirects TEXT NOT NULL,
      created TEXT NOT NULL DEFAULT (datetime('now'))
    );
    CREATE TABLE IF NOT EXISTS mcp_refresh (
      hash TEXT PRIMARY KEY,
      user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
      client_id TEXT NOT NULL,
      token_id INTEGER NOT NULL,
      created TEXT NOT NULL DEFAULT (datetime('now'))
    );
  `);

  /** Коды входа и ждущие подтверждения запросы — в памяти: живут минуты. */
  const codes = new Map();
  const pending = new Map();
  const registered = new Map();
  setInterval(() => {
    const now = Date.now();
    for (const [key, value] of codes) if (now - value.at > CODE_TTL) codes.delete(key);
    for (const [key, value] of pending) if (now - value.at > PENDING_TTL) pending.delete(key);
  }, 60_000).unref();

  const resourceMeta = () => ({
    resource: `${publicUrl}/mcp`,
    authorization_servers: [publicUrl],
    bearer_methods_supported: ["header"],
    resource_name: "NOAH",
  });
  const serverMeta = () => ({
    issuer: publicUrl,
    authorization_endpoint: `${publicUrl}/oauth/authorize`,
    token_endpoint: `${publicUrl}/oauth/token`,
    registration_endpoint: `${publicUrl}/oauth/register`,
    response_types_supported: ["code"],
    grant_types_supported: ["authorization_code", "refresh_token"],
    code_challenge_methods_supported: ["S256"],
    token_endpoint_auth_methods_supported: ["none"],
    scopes_supported: ["mcp"],
  });

  const json = (res, status, body, headers = {}) => {
    res.writeHead(status, { "Content-Type": "application/json; charset=utf-8", "Cache-Control": "no-store", "Access-Control-Allow-Origin": "*", ...headers });
    res.end(JSON.stringify(body));
  };
  const oauthError = (res, error, description, status = 400) => json(res, status, { error, error_description: description });

  async function readBody(req) {
    const chunks = [];
    let size = 0;
    for await (const chunk of req) {
      size += chunk.length;
      if (size > 64 * 1024) throw new Error("слишком большой запрос");
      chunks.push(chunk);
    }
    const text = Buffer.concat(chunks).toString("utf8");
    const type = String(req.headers["content-type"] ?? "");
    if (type.includes("application/json")) return JSON.parse(text || "{}");
    return Object.fromEntries(new URLSearchParams(text));
  }

  /** Выдаёт доступ: ключ площадки и refresh-токен к нему. */
  function grant(userId, clientId, clientName) {
    const access = `noah_${b64url(randomBytes(24))}`;
    const refresh = `noahr_${b64url(randomBytes(32))}`;
    const info = db.prepare("INSERT INTO tokens (user_id, label, hash) VALUES (?, ?, ?)").run(userId, `${clientName} — вход`.slice(0, 40), sha(access));
    db.prepare("INSERT INTO mcp_refresh (hash, user_id, client_id, token_id) VALUES (?, ?, ?, ?)").run(sha(refresh), userId, clientId, Number(info.lastInsertRowid));
    return { access_token: access, token_type: "Bearer", expires_in: ACCESS_DAYS * 86400, refresh_token: refresh, scope: "mcp" };
  }

  function page(res, title, body, status = 200) {
    res.writeHead(status, {
      "Content-Type": "text/html; charset=utf-8",
      "Cache-Control": "no-store",
      "Content-Security-Policy": "default-src 'self'; style-src 'self' 'unsafe-inline' https://fonts.googleapis.com; font-src https://fonts.gstatic.com; form-action 'self' https: http://localhost:* http://127.0.0.1:*; frame-ancestors 'none'",
      "X-Frame-Options": "DENY",
    });
    res.end(`<!doctype html><html lang="ru"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>${escape(title)} — NOAH</title><link rel="stylesheet" href="/styles.css">
<style>.oauth{max-width:520px;margin:10vh auto;padding:28px}.oauth h1{margin:0 0 12px}.oauth .row{display:flex;gap:12px;margin-top:22px;flex-wrap:wrap}.oauth .warn{color:var(--danger,#e2705f)}</style>
</head><body><main class="oauth plate">${body}</main></body></html>`);
  }

  function linkedNote(userId) {
    const row = db.prepare("SELECT seen FROM devices WHERE user_id = ?").get(userId);
    if (row?.seen) {
      const days = Math.round((Date.now() - row.seen) / 86400e3);
      return `<p class="hint">NOAH на компьютере связан с этим аккаунтом${days > 1 ? ` (последний раз на связи ${days} дн. назад)` : ""}: модули и курсы попадут к нему.</p>`;
    }
    return `<p class="warn">С этим аккаунтом NOAH на компьютере ещё не связан. Если ключ площадки в NOAH от другого аккаунта, выйдите и войдите в тот — иначе модули и курсы до NOAH не дойдут.</p>`;
  }

  /** Обработчик: true — запрос его. */
  return async function handle(req, res, url) {
    const path = url.pathname;

    if (req.method === "OPTIONS" && (path.startsWith("/.well-known/oauth") || path.startsWith("/oauth/"))) {
      res.writeHead(204, { "Access-Control-Allow-Origin": "*", "Access-Control-Allow-Headers": "content-type, authorization, mcp-protocol-version", "Access-Control-Allow-Methods": "GET, POST, OPTIONS" });
      res.end();
      return true;
    }
    if (path.startsWith("/.well-known/oauth-protected-resource")) {
      json(res, 200, resourceMeta());
      return true;
    }
    if (path.startsWith("/.well-known/oauth-authorization-server") || path.startsWith("/.well-known/openid-configuration")) {
      json(res, 200, serverMeta());
      return true;
    }

    if (path === "/oauth/register" && req.method === "POST") {
      const ip = String(req.headers["x-real-ip"] ?? req.socket.remoteAddress ?? "");
      const recent = (registered.get(ip) ?? []).filter((at) => Date.now() - at < 3600e3);
      if (recent.length >= REGISTER_PER_HOUR) {
        oauthError(res, "slow_down", "Слишком много регистраций — попробуйте через час.", 429);
        return true;
      }
      registered.set(ip, [...recent, Date.now()]);
      let body;
      try {
        body = await readBody(req);
      } catch (err) {
        oauthError(res, "invalid_client_metadata", err.message);
        return true;
      }
      const redirects = (Array.isArray(body.redirect_uris) ? body.redirect_uris : []).map(String).filter((uri) => /^(https:\/\/|http:\/\/(localhost|127\.0\.0\.1)(:\d+)?\/)/.test(uri));
      if (!redirects.length) {
        oauthError(res, "invalid_redirect_uri", "Нужен хотя бы один redirect_uri по https или на localhost.");
        return true;
      }
      const id = `mcp_${b64url(randomBytes(16))}`;
      const name = String(body.client_name ?? "Нейросеть").slice(0, 60);
      db.prepare("INSERT INTO mcp_clients (id, name, redirects) VALUES (?, ?, ?)").run(id, name, JSON.stringify(redirects.slice(0, 10)));
      json(res, 201, {
        client_id: id,
        client_name: name,
        redirect_uris: redirects,
        grant_types: ["authorization_code", "refresh_token"],
        response_types: ["code"],
        token_endpoint_auth_method: "none",
      });
      return true;
    }

    if (path === "/oauth/authorize" && req.method === "GET") {
      const q = Object.fromEntries(url.searchParams);
      const client = db.prepare("SELECT id, name, redirects FROM mcp_clients WHERE id = ?").get(String(q.client_id ?? ""));
      const redirects = client ? JSON.parse(client.redirects) : [];
      if (!client || !redirects.includes(q.redirect_uri)) {
        page(res, "Ошибка входа", `<h1>Не получилось</h1><p>Нейросеть прислала неизвестный адрес возврата. Удалите подключение NOAH в нейросети и добавьте заново.</p>`, 400);
        return true;
      }
      if (q.response_type !== "code" || q.code_challenge_method !== "S256" || !q.code_challenge) {
        const back = new URL(q.redirect_uri);
        back.searchParams.set("error", "invalid_request");
        if (q.state) back.searchParams.set("state", q.state);
        res.writeHead(302, { Location: back.toString() });
        res.end();
        return true;
      }
      const user = sessionUser(req);
      if (!user) {
        // Вход на сайте: после него страница вернёт сюда же (см. app.js).
        const next = `${url.pathname}${url.search}`;
        res.writeHead(302, { Location: `/#/login?next=${encodeURIComponent(next)}`, "Cache-Control": "no-store" });
        res.end();
        return true;
      }
      // Куда уйдёт доступ — показываем адресом, а не только названием: название
      // клиент пишет о себе сам, и «Claude» может назваться кто угодно.
      const host = new URL(q.redirect_uri).hostname;
      const known = KNOWN_HOSTS.some((name) => host === name || host.endsWith(`.${name}`));
      const ticket = b64url(randomBytes(18));
      pending.set(ticket, { at: Date.now(), userId: user.id, clientId: client.id, redirect: q.redirect_uri, challenge: q.code_challenge, state: q.state ?? "" });
      page(
        res,
        "Подключить нейросеть",
        `<h1>Подключить ${escape(client.name)} к NOAH?</h1>
<p>Нейросеть сможет собирать для вас модули и курсы и отправлять их в NOAH — а модули запускаются на вашем компьютере. Аккаунт: <strong>${escape(user.name)}</strong>.</p>
<p>Доступ получит: <strong>${escape(host)}</strong>.</p>
${known ? "" : `<p class="warn">Это не Claude, ChatGPT и не программа на вашем компьютере. Разрешайте, только если сами подключали эту нейросеть прямо сейчас.</p>`}
${linkedNote(user.id)}
<form method="post" action="/oauth/authorize" class="row">
<input type="hidden" name="ticket" value="${ticket}">
<button class="btn btn--gold" name="decision" value="allow">Разрешить</button>
<button class="btn" name="decision" value="deny">Отказать</button>
</form>
<p class="hint">Отозвать доступ можно в кабинете, в разделе ключей.</p>`,
      );
      return true;
    }

    if (path === "/oauth/authorize" && req.method === "POST") {
      const body = await readBody(req).catch(() => ({}));
      const entry = pending.get(String(body.ticket ?? ""));
      pending.delete(String(body.ticket ?? ""));
      const user = sessionUser(req);
      if (!entry || !user || user.id !== entry.userId) {
        page(res, "Ошибка входа", "<h1>Ссылка устарела</h1><p>Начните подключение в нейросети ещё раз.</p>", 400);
        return true;
      }
      const back = new URL(entry.redirect);
      if (body.decision !== "allow") {
        back.searchParams.set("error", "access_denied");
      } else {
        const code = b64url(randomBytes(24));
        codes.set(code, { ...entry, at: Date.now() });
        back.searchParams.set("code", code);
      }
      if (entry.state) back.searchParams.set("state", entry.state);
      res.writeHead(302, { Location: back.toString() });
      res.end();
      return true;
    }

    if (path === "/oauth/token" && req.method === "POST") {
      let body;
      try {
        body = await readBody(req);
      } catch (err) {
        oauthError(res, "invalid_request", err.message);
        return true;
      }
      if (body.grant_type === "authorization_code") {
        const entry = codes.get(String(body.code ?? ""));
        codes.delete(String(body.code ?? ""));
        if (!entry || entry.clientId !== body.client_id || entry.redirect !== body.redirect_uri) {
          oauthError(res, "invalid_grant", "Код входа неверный или устарел.");
          return true;
        }
        const verifier = String(body.code_verifier ?? "");
        if (b64url(createHash("sha256").update(verifier).digest()) !== entry.challenge) {
          oauthError(res, "invalid_grant", "code_verifier не совпал.");
          return true;
        }
        const client = db.prepare("SELECT name FROM mcp_clients WHERE id = ?").get(entry.clientId);
        json(res, 200, grant(entry.userId, entry.clientId, client?.name ?? "Нейросеть"));
        return true;
      }
      if (body.grant_type === "refresh_token") {
        const row = db.prepare("SELECT user_id, client_id, token_id FROM mcp_refresh WHERE hash = ?").get(sha(String(body.refresh_token ?? "")));
        if (!row || (body.client_id && body.client_id !== row.client_id)) {
          oauthError(res, "invalid_grant", "Доступ отозван — подключите NOAH в нейросети заново.");
          return true;
        }
        // Прежний ключ остаётся рабочим: клиент мог не успеть его сменить, а
        // запрос, упавший посреди работы, хуже лишнего ключа в кабинете.
        db.prepare("DELETE FROM mcp_refresh WHERE hash = ?").run(sha(String(body.refresh_token)));
        const alive = db.prepare("SELECT id FROM tokens WHERE id = ?").get(row.token_id);
        if (!alive) {
          oauthError(res, "invalid_grant", "Доступ отозван в кабинете — подключите NOAH в нейросети заново.");
          return true;
        }
        const client = db.prepare("SELECT name FROM mcp_clients WHERE id = ?").get(row.client_id);
        json(res, 200, grant(row.user_id, row.client_id, client?.name ?? "Нейросеть"));
        return true;
      }
      oauthError(res, "unsupported_grant_type", "Поддерживаются authorization_code и refresh_token.");
      return true;
    }
    return false;
  };
}

/** Заголовок ответа 401 для `/mcp`: где клиенту найти вход. */
export function authChallenge(publicUrl) {
  return `Bearer resource_metadata="${publicUrl}/.well-known/oauth-protected-resource"`;
}
