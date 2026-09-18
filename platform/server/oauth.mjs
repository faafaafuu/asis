// Вход через Google, GitHub, X, Яндекс, VK и Telegram.
//
// Провайдер включается, когда в окружении сервиса есть его ключи
// (см. platform/README.md). Без ключей кнопки на сайте не появляются.
// Вход через Telegram идёт через бота площадки и не требует домена:
// человек жмёт «Старт» у бота, сайт видит подтверждение и впускает его.

import { createHash, randomBytes } from "node:crypto";

const PUBLIC_URL = (process.env.NOAH_PUBLIC_URL ?? "http://84.247.166.53:8795").replace(/\/$/, "");
const STATE_TTL = 10 * 60_000;
const TG_TTL = 5 * 60_000;

const env = (name) => process.env[name]?.trim() || "";
const b64url = (buf) => Buffer.from(buf).toString("base64url");
const challenge = (verifier) => b64url(createHash("sha256").update(verifier).digest());

const PROVIDERS = {
  google: {
    title: "Google",
    id: env("NOAH_GOOGLE_ID"),
    secret: env("NOAH_GOOGLE_SECRET"),
    authorize: "https://accounts.google.com/o/oauth2/v2/auth",
    token: "https://oauth2.googleapis.com/token",
    scope: "openid email profile",
    pkce: true,
    async profile(token) {
      const me = await getJson("https://openidconnect.googleapis.com/v1/userinfo", { Authorization: `Bearer ${token.access_token}` });
      return { subject: me.sub, email: me.email_verified ? me.email : "", name: me.name || me.email };
    },
  },
  github: {
    title: "GitHub",
    id: env("NOAH_GITHUB_ID"),
    secret: env("NOAH_GITHUB_SECRET"),
    authorize: "https://github.com/login/oauth/authorize",
    token: "https://github.com/login/oauth/access_token",
    scope: "read:user user:email",
    async profile(token) {
      const headers = { Authorization: `Bearer ${token.access_token}`, "User-Agent": "noah-platform" };
      const me = await getJson("https://api.github.com/user", headers);
      const emails = await getJson("https://api.github.com/user/emails", headers).catch(() => []);
      const primary = Array.isArray(emails) ? emails.find((e) => e.primary && e.verified) : null;
      return { subject: String(me.id), email: primary?.email ?? "", name: me.login };
    },
  },
  x: {
    title: "X",
    id: env("NOAH_X_ID"),
    secret: env("NOAH_X_SECRET"),
    authorize: "https://x.com/i/oauth2/authorize",
    token: "https://api.x.com/2/oauth2/token",
    scope: "users.read tweet.read",
    pkce: true,
    basic: true,
    async profile(token) {
      const me = await getJson("https://api.x.com/2/users/me", { Authorization: `Bearer ${token.access_token}` });
      return { subject: me.data.id, email: "", name: me.data.username };
    },
  },
  yandex: {
    title: "Яндекс",
    id: env("NOAH_YANDEX_ID"),
    secret: env("NOAH_YANDEX_SECRET"),
    authorize: "https://oauth.yandex.ru/authorize",
    token: "https://oauth.yandex.ru/token",
    scope: "login:email login:info",
    async profile(token) {
      const me = await getJson("https://login.yandex.ru/info?format=json", { Authorization: `OAuth ${token.access_token}` });
      return { subject: String(me.id), email: me.default_email ?? "", name: me.login };
    },
  },
  vk: {
    title: "VK",
    id: env("NOAH_VK_ID"),
    secret: env("NOAH_VK_SECRET"),
    authorize: "https://id.vk.com/authorize",
    token: "https://id.vk.com/oauth2/auth",
    scope: "email",
    pkce: true,
    vk: true,
    untrustedEmail: true,
    async profile(token) {
      const body = new URLSearchParams({ client_id: this.id, access_token: token.access_token });
      const me = await postForm("https://id.vk.com/oauth2/user_info", body);
      return { subject: String(me.user.user_id), email: me.user.email ?? "", name: [me.user.first_name, me.user.last_name].filter(Boolean).join(" ") };
    },
  },
};

async function getJson(url, headers) {
  const response = await fetch(url, { headers: { Accept: "application/json", ...headers }, signal: AbortSignal.timeout(15000) });
  if (!response.ok) throw new Error(`${new URL(url).host} ответил ${response.status}`);
  return response.json();
}

async function postForm(url, body, headers = {}) {
  const response = await fetch(url, {
    method: "POST",
    headers: { "Content-Type": "application/x-www-form-urlencoded", Accept: "application/json", ...headers },
    body,
    signal: AbortSignal.timeout(15000),
  });
  const data = await response.json().catch(() => ({}));
  if (!response.ok || data.error) throw new Error(data.error_description || data.error || `${new URL(url).host} ответил ${response.status}`);
  return data;
}

export function mountOAuth({ route, db, Fail, readJson, sessionUser, openSession, cookies }) {
  db.exec(`
    CREATE TABLE IF NOT EXISTS identities (
      provider TEXT NOT NULL,
      subject TEXT NOT NULL,
      user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
      created TEXT NOT NULL DEFAULT (datetime('now')),
      PRIMARY KEY (provider, subject)
    );
  `);

  const states = new Map();
  const tgCodes = new Map();
  const tgBot = { token: env("NOAH_TG_BOT_TOKEN"), name: env("NOAH_TG_BOT_NAME").replace(/^@/, "") };
  const enabled = () => [
    ...Object.entries(PROVIDERS)
      .filter(([, p]) => p.id && p.secret)
      .map(([key, p]) => ({ id: key, title: p.title })),
    ...(tgBot.token && tgBot.name ? [{ id: "telegram", title: "Telegram" }] : []),
  ];

  /** Имя автора из имени профиля: латиница, цифры, точка, дефис; свободное. */
  const freeName = (raw) => {
    const translit = { а: "a", б: "b", в: "v", г: "g", д: "d", е: "e", ё: "e", ж: "zh", з: "z", и: "i", й: "y", к: "k", л: "l", м: "m", н: "n", о: "o", п: "p", р: "r", с: "s", т: "t", у: "u", ф: "f", х: "h", ц: "c", ч: "ch", ш: "sh", щ: "sch", ы: "y", э: "e", ю: "yu", я: "ya" };
    let base = String(raw ?? "")
      .toLowerCase()
      .split("")
      .map((ch) => translit[ch] ?? ch)
      .join("")
      .replace(/[^a-z0-9._-]+/g, "-")
      .replace(/^[-._]+|[-._]+$/g, "")
      .slice(0, 18);
    if (base.length < 3) base = `user-${base}`.slice(0, 18);
    let name = base;
    for (let i = 2; db.prepare("SELECT 1 FROM users WHERE name = ?").get(name); i++) name = `${base}-${i}`;
    return name;
  };

  /** Находит или заводит аккаунт по внешнему профилю и открывает сессию. */
  const signIn = (req, provider, profile, trustEmail = true) => {
    const current = sessionUser(req);
    const linked = db.prepare("SELECT user_id FROM identities WHERE provider = ? AND subject = ?").get(provider, profile.subject);
    let userId = linked?.user_id;
    if (!userId && current) userId = current.id;
    const taken = profile.email && db.prepare("SELECT id FROM users WHERE email = ?").get(profile.email)?.id;
    if (!userId && taken && trustEmail) userId = taken;
    if (!userId) {
      const email = profile.email && !taken ? profile.email : `${provider}-${profile.subject}@noreply.noah`;
      const info = db.prepare("INSERT INTO users (email, name, pass) VALUES (?, ?, ?)").run(email, freeName(profile.name), `oauth$${provider}`);
      userId = Number(info.lastInsertRowid);
    }
    db.prepare("INSERT OR IGNORE INTO identities (provider, subject, user_id) VALUES (?, ?, ?)").run(provider, profile.subject, userId);
    return openSession(userId);
  };

  route("GET", /^\/api\/auth\/providers$/, () => ({ providers: enabled() }));

  /* ── Способы входа в кабинете ──────────────────────────────────────────── */

  const methods = (userId) => {
    const hasPassword = !db.prepare("SELECT pass FROM users WHERE id = ?").get(userId).pass.startsWith("oauth$");
    const linked = db.prepare("SELECT provider, created FROM identities WHERE user_id = ? ORDER BY created").all(userId);
    return { hasPassword, linked };
  };

  route("GET", /^\/api\/my\/identities$/, ({ req }) => {
    const user = sessionUser(req);
    if (!user) throw new Fail(401, "Войдите в аккаунт.");
    return methods(user.id);
  });

  route("DELETE", /^\/api\/my\/identities\/([a-z]+)$/, ({ req, match }) => {
    const user = sessionUser(req);
    if (!user) throw new Fail(401, "Войдите в аккаунт.");
    const { hasPassword, linked } = methods(user.id);
    if (!linked.some((row) => row.provider === match[1])) throw new Fail(404, "Этот способ входа не привязан.");
    if (!hasPassword && linked.length === 1) throw new Fail(400, "Это единственный способ входа. Сначала задайте пароль.");
    db.prepare("DELETE FROM identities WHERE user_id = ? AND provider = ?").run(user.id, match[1]);
    return methods(user.id);
  });

  /* ── OAuth: уход к провайдеру и возвращение ────────────────────────────── */

  const start = (res, key) => {
    const p = PROVIDERS[key];
    if (!p?.id || !p.secret) throw new Fail(404, "Этот способ входа не настроен.");
    const state = b64url(randomBytes(24));
    const verifier = b64url(randomBytes(48));
    states.set(state, { key, verifier, at: Date.now() });
    const params = new URLSearchParams({
      response_type: "code",
      client_id: p.id,
      redirect_uri: `${PUBLIC_URL}/auth/${key}/callback`,
      scope: p.scope,
      state,
    });
    if (p.pkce) {
      params.set("code_challenge", challenge(verifier));
      params.set("code_challenge_method", "S256");
    }
    res.writeHead(302, {
      Location: `${p.authorize}?${params}`,
      "Set-Cookie": `noah_oauth=${state}; Path=/auth; HttpOnly; SameSite=Lax; Max-Age=600`,
    });
    res.end();
  };

  const finish = async (req, res, url, key) => {
    const p = PROVIDERS[key];
    const state = url.searchParams.get("state") ?? "";
    const saved = states.get(state);
    states.delete(state);
    const back = (message) => {
      res.writeHead(302, { Location: `/#/login?error=${encodeURIComponent(message)}` });
      res.end();
    };
    if (!p || !saved || saved.key !== key || Date.now() - saved.at > STATE_TTL || cookies(req).noah_oauth !== state) {
      return back("Вход прервался — попробуйте ещё раз.");
    }
    if (url.searchParams.get("error")) return back("Вход отменён.");
    try {
      const body = new URLSearchParams({
        grant_type: "authorization_code",
        code: url.searchParams.get("code") ?? "",
        redirect_uri: `${PUBLIC_URL}/auth/${key}/callback`,
        client_id: p.id,
      });
      const headers = {};
      if (p.basic) headers.Authorization = `Basic ${Buffer.from(`${p.id}:${p.secret}`).toString("base64")}`;
      else body.set("client_secret", p.secret);
      if (p.pkce) body.set("code_verifier", saved.verifier);
      if (p.vk) {
        body.set("device_id", url.searchParams.get("device_id") ?? "");
        body.set("state", state);
      }
      const token = await postForm(p.token, body, headers);
      const profile = await p.profile(token);
      if (!profile.subject) throw new Error("провайдер не прислал профиль");
      const linking = Boolean(sessionUser(req));
      const cookie = signIn(req, key, profile, !p.untrustedEmail);
      res.writeHead(302, { Location: linking ? "/#/account" : "/#/connect", "Set-Cookie": cookie });
      res.end();
    } catch (err) {
      console.error(`вход через ${key}:`, err.message);
      back(`Не получилось войти через ${p.title}.`);
    }
  };

  /* ── Telegram: через бота площадки ─────────────────────────────────────── */

  route("POST", /^\/api\/auth\/telegram\/start$/, ({ res }) => {
    if (!tgBot.token || !tgBot.name) throw new Fail(404, "Вход через Telegram не настроен.");
    const code = b64url(randomBytes(12)).replace(/[^A-Za-z0-9]/g, "").slice(0, 16);
    tgCodes.set(code, { at: Date.now(), user: null });
    res.setHeader("Set-Cookie", `noah_tg=${code}; Path=/; HttpOnly; SameSite=Lax; Max-Age=300`);
    return { link: `https://t.me/${tgBot.name}?start=${code}` };
  });

  route("GET", /^\/api\/auth\/telegram\/status$/, ({ req, res }) => {
    const code = cookies(req).noah_tg;
    const entry = code && tgCodes.get(code);
    if (!entry || Date.now() - entry.at > TG_TTL) throw new Fail(410, "Ссылка устарела — нажмите «Войти через Telegram» ещё раз.");
    if (!entry.user) return { done: false };
    tgCodes.delete(code);
    res.setHeader("Set-Cookie", [signIn(req, "telegram", entry.user), "noah_tg=; Path=/; Max-Age=0"]);
    return { done: true };
  });

  if (tgBot.token && tgBot.name) {
    let offset = 0;
    const poll = async () => {
      try {
        const response = await fetch(`https://api.telegram.org/bot${tgBot.token}/getUpdates?timeout=25&offset=${offset}`, { signal: AbortSignal.timeout(35000) });
        const data = await response.json();
        for (const update of data.result ?? []) {
          offset = update.update_id + 1;
          const message = update.message;
          if (!message?.chat?.id) continue;
          const match = /^\/start\s+([A-Za-z0-9]{8,32})$/.exec(message?.text ?? "");
          const entry = match && tgCodes.get(match[1]);
          let reply = "Это бот входа на площадку NOAH. Нажмите «Войти через Telegram» на сайте.";
          if (entry && Date.now() - entry.at < TG_TTL) {
            const from = message.from;
            entry.user = { subject: String(from.id), email: "", name: from.username || [from.first_name, from.last_name].filter(Boolean).join(" ") };
            reply = "Готово — вернитесь на сайт NOAH, вход уже выполнен.";
          }
          await fetch(`https://api.telegram.org/bot${tgBot.token}/sendMessage`, {
            method: "POST",
            headers: { "Content-Type": "application/json" },
            body: JSON.stringify({ chat_id: message?.chat?.id, text: reply }),
          }).catch(() => {});
        }
      } catch {
        await new Promise((resolve) => setTimeout(resolve, 5000));
      }
      setImmediate(poll);
    };
    poll();
  }

  setInterval(() => {
    const now = Date.now();
    for (const [key, value] of states) if (now - value.at > STATE_TTL) states.delete(key);
    for (const [key, value] of tgCodes) if (now - value.at > TG_TTL) tgCodes.delete(key);
  }, 60_000).unref();

  return async function oauth(req, res, url) {
    const match = /^\/auth\/(google|github|x|yandex|vk)(\/callback)?$/.exec(url.pathname);
    if (!match || req.method !== "GET") return false;
    if (match[2]) await finish(req, res, url, match[1]);
    else start(res, match[1]);
    return true;
  };
}
