// Площадка NOAH: библиотека модулей, аккаунты, публикация из Ноа.
//
// Без зависимостей: node:http, node:sqlite, node:crypto. Код модулей сервер не
// запускает никогда — модуль проверяет Ноа у автора перед публикацией и Ноа у
// каждого пользователя перед установкой.

import http from "node:http";
import net from "node:net";
import tls from "node:tls";
import { Duplex } from "node:stream";
import { readFile, stat } from "node:fs/promises";
import { existsSync, mkdirSync, readdirSync, readFileSync, statSync } from "node:fs";
import { dirname, extname, join, normalize, sep } from "node:path";
import { fileURLToPath } from "node:url";
import { createHash, randomBytes, scrypt as scryptCb, timingSafeEqual } from "node:crypto";
import { promisify } from "node:util";
import { brotliCompressSync, constants as zlibConstants, gzipSync } from "node:zlib";
import { DatabaseSync } from "node:sqlite";

import { CATEGORIES, FORMAT, lint, validId } from "./standard.mjs";
import { mountRemote } from "./remote.mjs";
import { mountOAuth } from "./oauth.mjs";
import { mountMcpAuth } from "./mcpauth.mjs";
import { mountNoa } from "./noa.mjs";
import { tunnel } from "./tunnel.mjs";

const scrypt = promisify(scryptCb);
const HERE = dirname(fileURLToPath(import.meta.url));
const WEB = normalize(join(HERE, "..", "web"));
// Ноа в браузере — те же окна, что в программе: они лежат в src/ репозитория
// и раздаются по адресу /app/.
const APP = normalize(join(HERE, "..", "..", "src"));
/** Что из src/ можно отдавать: страницы Ноа онлайн, их скрипты, стили, шрифты. */
const APP_FILES = /^\/(app\.html|learning\.html|(js|styles|assets)\/[\w./-]+)$/;
const PORT = Number(process.env.NOAH_PORT ?? 8795);
const HOST = process.env.NOAH_HOST ?? "0.0.0.0";
const DATA = process.env.NOAH_DATA ?? join(HERE, "..", "data");
const SEED = process.env.NOAH_SEED ?? join(HERE, "..", "..", "modules", "index.json");
const BUILTIN = process.env.NOAH_BUILTIN ?? join(HERE, "..", "..", "modules", "builtin.json");
const SECURE = process.env.NOAH_SECURE === "1";
/**
 * Зеркала сайта — те же страницы под другим именем, например m.noahlab.ru
 * через российский CDN: с мобильного интернета в РФ зарубежный сервер под
 * своим именем режется, а через CDN открывается. Cookie входа — на весь
 * домен, чтобы вход на одном имени действовал и на другом.
 */
const COOKIE_DOMAIN = (process.env.NOAH_COOKIE_DOMAIN ?? "").trim();
const MIRRORS = new Set(
  String(process.env.NOAH_MIRRORS ?? "")
    .split(",")
    .map((host) => host.trim().toLowerCase())
    .filter(Boolean),
);
const cookieDomain = COOKIE_DOMAIN ? `; Domain=${COOKIE_DOMAIN}` : "";

/** Подходит ли общий домен cookie к адресу запроса (по IP — нет). */
function cookieFits(req) {
  const host = String(req.headers.host ?? "").toLowerCase().replace(/:\d+$/, "");
  return host === COOKIE_DOMAIN || host.endsWith(`.${COOKIE_DOMAIN}`);
}

/**
 * Убирает Domain из cookie ответа. Браузер отбрасывает cookie с чужим
 * доменом, и по IP вход не держался бы.
 */
function dropCookieDomain(res) {
  const strip = (value) => (Array.isArray(value) ? value.map(strip) : typeof value === "string" ? value.replace(/;\s*Domain=[^;]*/i, "") : value);
  const setHeader = res.setHeader.bind(res);
  res.setHeader = (name, value) => setHeader(name, String(name).toLowerCase() === "set-cookie" ? strip(value) : value);
  const writeHead = res.writeHead.bind(res);
  res.writeHead = (status, ...rest) => {
    const headers = rest.at(-1);
    if (headers && typeof headers === "object" && !Array.isArray(headers)) {
      for (const name of Object.keys(headers)) if (name.toLowerCase() === "set-cookie") headers[name] = strip(headers[name]);
    }
    return writeHead(status, ...rest);
  };
}

/** С какого зеркала пришёл человек: `https://m.…` или пусто — с основного адреса. */
function mirrorOrigin(req) {
  for (const header of [req.headers.origin, req.headers.referer]) {
    try {
      const url = new URL(String(header ?? ""));
      if (MIRRORS.has(url.host.toLowerCase())) return `https://${url.host.toLowerCase()}`;
    } catch {
      /* нет заголовка или он кривой */
    }
  }
  return "";
}
const SESSION_DAYS = 30;
const MAX_BODY = 8 * 1024 * 1024;

mkdirSync(DATA, { recursive: true });
const db = new DatabaseSync(join(DATA, "noah.db"));
db.exec(`
  PRAGMA journal_mode = WAL;
  PRAGMA foreign_keys = ON;
  CREATE TABLE IF NOT EXISTS users (
    id INTEGER PRIMARY KEY,
    email TEXT NOT NULL UNIQUE COLLATE NOCASE,
    name TEXT NOT NULL UNIQUE COLLATE NOCASE,
    pass TEXT NOT NULL,
    created TEXT NOT NULL DEFAULT (datetime('now'))
  );
  CREATE TABLE IF NOT EXISTS sessions (
    hash TEXT PRIMARY KEY,
    user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    expires TEXT NOT NULL
  );
  CREATE TABLE IF NOT EXISTS tokens (
    id INTEGER PRIMARY KEY,
    user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    label TEXT NOT NULL,
    hash TEXT NOT NULL UNIQUE,
    created TEXT NOT NULL DEFAULT (datetime('now')),
    used TEXT
  );
  CREATE TABLE IF NOT EXISTS modules (
    id TEXT PRIMARY KEY,
    owner_id INTEGER REFERENCES users(id) ON DELETE SET NULL,
    author TEXT NOT NULL,
    title TEXT NOT NULL,
    icon TEXT NOT NULL,
    about TEXT NOT NULL,
    voice TEXT NOT NULL,
    category TEXT NOT NULL DEFAULT 'other',
    version TEXT NOT NULL DEFAULT '1.0.0',
    description TEXT NOT NULL DEFAULT '',
    manifest TEXT NOT NULL,
    files TEXT NOT NULL DEFAULT '{}',
    tools TEXT NOT NULL DEFAULT '[]',
    installs INTEGER NOT NULL DEFAULT 0,
    hidden INTEGER NOT NULL DEFAULT 0,
    created TEXT NOT NULL DEFAULT (datetime('now')),
    updated TEXT NOT NULL DEFAULT (datetime('now'))
  );
`);

/* ── Библиотека ядра ─────────────────────────────────────────────────────── */

const CORE_CATEGORY = { memory: "work", files: "work", fetch: "work", browser: "work", docs: "dev", thinking: "work" };
/** Модули ядра, которым не нужна сеть. */
const CORE_LOCAL = ["memory", "files", "thinking"];
function seed() {
  if (!existsSync(SEED)) return;
  const body = JSON.parse(readFileSync(SEED, "utf8"));
  const upsert = db.prepare(`
    INSERT INTO modules (id, owner_id, author, title, icon, about, voice, category, manifest)
    VALUES (?, NULL, 'noah-core', ?, ?, ?, ?, ?, ?)
    ON CONFLICT(id) DO UPDATE SET title = excluded.title, icon = excluded.icon, about = excluded.about,
      voice = excluded.voice, manifest = excluded.manifest, updated = datetime('now')
    WHERE modules.owner_id IS NULL`);
  for (const m of body.modules ?? []) {
    if (!validId(m.id)) continue;
    const manifest = { ...m, local: CORE_LOCAL.includes(m.id) };
    upsert.run(m.id, m.title, m.icon, m.about, m.voice, CORE_CATEGORY[m.id] ?? "other", JSON.stringify(manifest));
  }
}
seed();

/** Модули, встроенные в само приложение: в библиотеке для обзора, ставятся вместе с NOAH. */
function seedBuiltin() {
  if (!existsSync(BUILTIN)) return;
  const body = JSON.parse(readFileSync(BUILTIN, "utf8"));
  const upsert = db.prepare(`
    INSERT INTO modules (id, owner_id, author, title, icon, about, voice, category, description, manifest, tools)
    VALUES (?, NULL, 'noah', ?, ?, ?, ?, ?, ?, ?, ?)
    ON CONFLICT(id) DO UPDATE SET title = excluded.title, icon = excluded.icon, about = excluded.about,
      voice = excluded.voice, category = excluded.category, description = excluded.description,
      manifest = excluded.manifest, tools = excluded.tools, updated = datetime('now')
    WHERE modules.owner_id IS NULL`);
  for (const m of body.modules ?? []) {
    if (!validId(m.id)) continue;
    const manifest = { id: m.id, title: m.title, icon: m.icon, about: m.about, voice: m.voice, builtin: true, local: true };
    upsert.run(m.id, m.title, m.icon, m.about, m.voice, m.category ?? "other", m.description ?? "", JSON.stringify(manifest), JSON.stringify(m.tools ?? []));
  }
}
seedBuiltin();

/* ── Пароли, сессии, токены ──────────────────────────────────────────────── */

const sha = (text) => createHash("sha256").update(text).digest("hex");

async function hashPassword(password) {
  const salt = randomBytes(16);
  const key = await scrypt(password, salt, 64, { N: 16384, r: 8, p: 1 });
  return `scrypt$${salt.toString("base64")}$${key.toString("base64")}`;
}

async function checkPassword(password, stored) {
  const [kind, salt, key] = String(stored).split("$");
  if (kind !== "scrypt") return false;
  const expected = Buffer.from(key, "base64");
  const actual = await scrypt(password, Buffer.from(salt, "base64"), expected.length, { N: 16384, r: 8, p: 1 });
  return timingSafeEqual(expected, actual);
}

function cookies(req) {
  const out = {};
  for (const part of String(req.headers.cookie ?? "").split(";")) {
    const at = part.indexOf("=");
    if (at > 0) out[part.slice(0, at).trim()] = decodeURIComponent(part.slice(at + 1).trim());
  }
  return out;
}

function sessionCookie(value, maxAge) {
  return [`noah_session=${value}`, "Path=/", "HttpOnly", "SameSite=Lax", `Max-Age=${maxAge}`, COOKIE_DOMAIN ? `Domain=${COOKIE_DOMAIN}` : "", SECURE ? "Secure" : ""]
    .filter(Boolean)
    .join("; ");
}

function openSession(userId) {
  const token = randomBytes(32).toString("base64url");
  const expires = new Date(Date.now() + SESSION_DAYS * 86400e3).toISOString();
  db.prepare("INSERT INTO sessions (hash, user_id, expires) VALUES (?, ?, ?)").run(sha(token), userId, expires);
  return sessionCookie(token, SESSION_DAYS * 86400);
}

function sessionUser(req) {
  const token = cookies(req).noah_session;
  if (!token) return null;
  return (
    db
      .prepare(
        `SELECT users.id, users.email, users.name FROM sessions JOIN users ON users.id = sessions.user_id
         WHERE sessions.hash = ? AND sessions.expires > ?`,
      )
      .get(sha(token), new Date().toISOString()) ?? null
  );
}

function tokenUser(req) {
  const match = /^Bearer\s+(noah_[A-Za-z0-9_-]{20,})$/.exec(String(req.headers.authorization ?? ""));
  return match ? userForKey(match[1]) : null;
}

/** Владелец ключа площадки: `noah_…`. */
function userForKey(key) {
  if (!/^noah_[A-Za-z0-9_-]{20,}$/.test(String(key ?? ""))) return null;
  const row = db
    .prepare("SELECT users.id, users.email, users.name, tokens.id AS token_id FROM tokens JOIN users ON users.id = tokens.user_id WHERE tokens.hash = ?")
    .get(sha(key));
  if (row) db.prepare("UPDATE tokens SET used = datetime('now') WHERE id = ?").run(row.token_id);
  return row ?? null;
}

/** Попытки входа: не больше 8 за 10 минут с одного адреса. */
const attempts = new Map();
function throttled(ip) {
  const now = Date.now();
  const list = (attempts.get(ip) ?? []).filter((at) => now - at < 600e3);
  list.push(now);
  attempts.set(ip, list);
  return list.length > 8;
}

/* ── Ответы ─────────────────────────────────────────────────────────────── */

/**
 * Для Ноа онлайн: модель человек подключает сам, и страница ходит к ней
 * напрямую — к любому провайдеру по HTTPS или к Ollama на своём компьютере.
 * Всё остальное — как у сайта.
 */
const APP_CSP =
  "default-src 'self'; style-src 'self'; font-src 'self'; img-src 'self' data:; media-src 'self' blob: data:; " +
  "connect-src 'self' https: http://localhost:* http://127.0.0.1:*; frame-ancestors 'none'; base-uri 'none'; form-action 'self'";

const SECURITY_HEADERS = {
  "X-Content-Type-Options": "nosniff",
  "Referrer-Policy": "same-origin",
  "X-Frame-Options": "DENY",
  "Content-Security-Policy":
    "default-src 'self'; style-src 'self' https://fonts.googleapis.com; font-src https://fonts.gstatic.com; img-src 'self' data:; connect-src 'self'; frame-ancestors 'none'; base-uri 'none'; form-action 'self'",
  // Раз зашли по HTTPS — дальше браузер ходит только так, без шага через http.
  ...(SECURE ? { "Strict-Transport-Security": "max-age=31536000" } : {}),
};

class Fail extends Error {
  constructor(status, message) {
    super(message);
    this.status = status;
  }
}

/** Сжатые файлы сайта: жмутся один раз, пока файл не изменился. */
const packCache = new Map();

/**
 * Сжатие текста: ответы меньше и быстрее доходят через медленные сети. Brotli
 * — где браузер его понимает: на треть меньше gzip, а из части сетей
 * соединение с сервером замирает после ~16 КБ, и каждый килобайт на счету.
 * `key` — файл и время его изменения, чтобы не жать одно и то же заново.
 */
function packed(req, body, key) {
  const accepts = String(req.headers["accept-encoding"] ?? "");
  const encoding = /(^|[\s,])br(;|,|$)/.test(accepts) ? "br" : /gzip/.test(accepts) ? "gzip" : "";
  if (!encoding || body.length < 1024) return { body, headers: {} };
  const cacheKey = key && `${encoding}:${key}`;
  let out = cacheKey && packCache.get(cacheKey);
  if (!out) {
    out =
      encoding === "br"
        ? brotliCompressSync(body, { params: { [zlibConstants.BROTLI_PARAM_QUALITY]: key ? 11 : 5 } })
        : gzipSync(body, { level: 9 });
    if (cacheKey) {
      // Старые версии файлов после выкладки больше не спросят — не копим их.
      if (packCache.size >= 400) packCache.delete(packCache.keys().next().value);
      packCache.set(cacheKey, out);
    }
  }
  return { body: out, headers: { "Content-Encoding": encoding, Vary: "Accept-Encoding" } };
}

function send(res, status, body, headers = {}) {
  const out = packed(res.req, Buffer.from(JSON.stringify(body)));
  res.writeHead(status, {
    ...SECURITY_HEADERS,
    "Content-Type": "application/json; charset=utf-8",
    "Cache-Control": "no-store",
    "Content-Length": out.body.length,
    ...out.headers,
    ...headers,
  });
  res.end(out.body);
}

async function readJson(req) {
  if (!String(req.headers["content-type"] ?? "").startsWith("application/json")) throw new Fail(415, "Нужен JSON.");
  const chunks = [];
  let size = 0;
  for await (const chunk of req) {
    size += chunk.length;
    if (size > MAX_BODY) throw new Fail(413, "Слишком большой запрос.");
    chunks.push(chunk);
  }
  try {
    return JSON.parse(Buffer.concat(chunks).toString("utf8") || "{}");
  } catch {
    throw new Fail(400, "Запрос не разобрался.");
  }
}

/** Изменяющие запросы из браузера — только со своей страницы. */
function sameOrigin(req) {
  const origin = req.headers.origin;
  if (!origin) return true;
  try {
    const host = new URL(origin).host.toLowerCase();
    return host === req.headers.host || MIRRORS.has(host);
  } catch {
    return false;
  }
}

function card(row) {
  return {
    id: row.id,
    title: row.title,
    icon: row.icon,
    about: row.about,
    voice: row.voice,
    author: row.author,
    category: row.category,
    version: row.version,
    installs: row.installs,
    updated: row.updated,
    core: row.owner_id === null,
    builtin: row.author === "noah" && row.owner_id === null,
  };
}

/* ── Маршруты ───────────────────────────────────────────────────────────── */

const routes = [];
const route = (method, pattern, handler) => routes.push({ method, pattern, handler });

route("GET", /^\/api\/health$/, () => ({ status: "ok" }));

route("GET", /^\/api\/me$/, ({ user }) => {
  if (!user) return { user: null };
  const row = db.prepare("SELECT pass FROM users WHERE id = ?").get(user.id);
  return { user: { ...user, hasPassword: String(row?.pass).startsWith("scrypt$") } };
});

route("POST", /^\/api\/auth\/register$/, async ({ req, res, ip }) => {
  if (throttled(ip)) throw new Fail(429, "Слишком много попыток. Подождите несколько минут.");
  const { email, name, password } = await readJson(req);
  if (!/^[^\s@]{1,64}@[^\s@]{1,190}\.[^\s@]{2,}$/.test(String(email ?? ""))) throw new Fail(400, "Проверьте почту.");
  if (!/^[a-z0-9][a-z0-9._-]{2,23}$/i.test(String(name ?? ""))) throw new Fail(400, "Имя автора — 3–24 знака: латиница, цифры, точка, дефис.");
  if (String(password ?? "").length < 10) throw new Fail(400, "Пароль — от 10 знаков.");
  if (String(name).toLowerCase() === "noah-core") throw new Fail(400, "Это имя занято.");
  const taken = db.prepare("SELECT email, name FROM users WHERE email = ? OR name = ?").get(email, name);
  if (taken) throw new Fail(409, taken.email.toLowerCase() === String(email).toLowerCase() ? "Эта почта уже зарегистрирована." : "Это имя уже занято.");
  const info = db.prepare("INSERT INTO users (email, name, pass) VALUES (?, ?, ?)").run(email.trim(), name.trim(), await hashPassword(password));
  res.setHeader("Set-Cookie", openSession(Number(info.lastInsertRowid)));
  return { user: { id: Number(info.lastInsertRowid), email: email.trim(), name: name.trim() } };
});

route("POST", /^\/api\/auth\/login$/, async ({ req, res, ip }) => {
  if (throttled(ip)) throw new Fail(429, "Слишком много попыток. Подождите несколько минут.");
  const { email, password } = await readJson(req);
  const row = db.prepare("SELECT id, email, name, pass FROM users WHERE email = ?").get(String(email ?? "").trim());
  // Проверка идёт и для несуществующей почты: время ответа не выдаёт, есть ли аккаунт.
  const ok = await checkPassword(String(password ?? ""), row?.pass ?? "scrypt$AAAAAAAAAAAAAAAAAAAAAA==$" + "A".repeat(86) + "==");
  if (!row || !ok) throw new Fail(401, "Неверная почта или пароль.");
  res.setHeader("Set-Cookie", openSession(row.id));
  return { user: { id: row.id, email: row.email, name: row.name } };
});

route("POST", /^\/api\/auth\/logout$/, ({ req, res }) => {
  const token = cookies(req).noah_session;
  if (token) db.prepare("DELETE FROM sessions WHERE hash = ?").run(sha(token));
  // Прежняя cookie — без домена: её тоже стираем, иначе выход не выходит.
  res.setHeader("Set-Cookie", [sessionCookie("", 0), "noah_session=; Path=/; Max-Age=0"]);
  return { ok: true };
});

route("POST", /^\/api\/account\/password$/, async ({ req, user, ip }) => {
  if (!user) throw new Fail(401, "Войдите.");
  if (throttled(ip)) throw new Fail(429, "Слишком много попыток. Подождите несколько минут.");
  const { current, next } = await readJson(req);
  const row = db.prepare("SELECT pass FROM users WHERE id = ?").get(user.id);
  const hasPassword = String(row.pass).startsWith("scrypt$");
  if (hasPassword && !(await checkPassword(String(current ?? ""), row.pass))) throw new Fail(403, "Текущий пароль не подходит.");
  if (String(next ?? "").length < 10) throw new Fail(400, "Пароль — от 10 знаков.");
  db.prepare("UPDATE users SET pass = ? WHERE id = ?").run(await hashPassword(next), user.id);
  return { ok: true };
});

route("POST", /^\/api\/account\/logout-others$/, ({ req, user }) => {
  if (!user) throw new Fail(401, "Войдите.");
  const token = cookies(req).noah_session;
  db.prepare("DELETE FROM sessions WHERE user_id = ? AND hash != ?").run(user.id, sha(token ?? ""));
  return { ok: true };
});

route("DELETE", /^\/api\/account$/, async ({ req, res, user, ip }) => {
  if (!user) throw new Fail(401, "Войдите.");
  if (throttled(ip)) throw new Fail(429, "Слишком много попыток. Подождите несколько минут.");
  const { password } = await readJson(req);
  const row = db.prepare("SELECT pass FROM users WHERE id = ?").get(user.id);
  const hasPassword = String(row.pass).startsWith("scrypt$");
  if (hasPassword && !(await checkPassword(String(password ?? ""), row.pass))) throw new Fail(403, "Пароль не подходит.");
  db.prepare("DELETE FROM modules WHERE owner_id = ?").run(user.id);
  db.prepare("DELETE FROM users WHERE id = ?").run(user.id);
  // Прежняя cookie — без домена: её тоже стираем, иначе выход не выходит.
  res.setHeader("Set-Cookie", [sessionCookie("", 0), "noah_session=; Path=/; Max-Age=0"]);
  return { ok: true };
});

route("GET", /^\/api\/stats$/, () => {
  const row = db
    .prepare("SELECT COUNT(*) AS modules, COUNT(DISTINCT author) AS authors, COALESCE(SUM(installs), 0) AS installs FROM modules WHERE hidden = 0")
    .get();
  // Мозги — источники, с которыми работает NOAH: Ollama, OpenRouter, Google AI Studio, Groq и любой OpenAI-совместимый.
  return { ...row, brains: 5, users: db.prepare("SELECT COUNT(*) AS n FROM users").get().n };
});

route("GET", /^\/api\/modules$/, ({ url }) => {
  const q = String(url.searchParams.get("q") ?? "").trim().toLowerCase();
  const category = url.searchParams.get("category");
  const sortKey = url.searchParams.get("sort");
  const sort = sortKey === "new" ? "created DESC, updated DESC" : "installs DESC, title";
  const rows = db.prepare(`SELECT * FROM modules WHERE hidden = 0 ORDER BY ${sort}`).all();
  return {
    modules: rows
      .filter((row) => {
        if (!category || category === "all" || category === "free") return true;
        if (category === "local") return JSON.parse(row.manifest).local === true;
        return row.category === category;
      })
      .filter((row) => !q || `${row.title} ${row.about} ${row.author} ${row.id}`.toLowerCase().includes(q))
      .map(card),
    categories: CATEGORIES,
  };
});

route("GET", /^\/api\/modules\/([a-z0-9-]+)$/, ({ match }) => {
  const row = db.prepare("SELECT * FROM modules WHERE id = ? AND hidden = 0").get(match[1]);
  if (!row) throw new Fail(404, "Модуль не найден.");
  const manifest = JSON.parse(row.manifest);
  return {
    ...card(row),
    description: row.description,
    tools: JSON.parse(row.tools),
    runtime: manifest.builtin ? "builtin" : (manifest.mcp?.command ?? ""),
    brain: typeof manifest.brain === "string" ? manifest.brain.slice(0, 40) : "",
    secrets: (manifest.secrets ?? []).map((s) => ({ title: s.title, hint: s.hint, optional: Boolean(s.optional) })),
    files: Object.keys(JSON.parse(row.files)),
  };
});

/** Пакет для установки в Ноа: описание и файлы. Ноа проверит его сама. */
route("GET", /^\/api\/modules\/([a-z0-9-]+)\/package$/, ({ match }) => {
  const row = db.prepare("SELECT * FROM modules WHERE id = ? AND hidden = 0").get(match[1]);
  if (!row) throw new Fail(404, "Модуль не найден.");
  if (JSON.parse(row.manifest).builtin) throw new Fail(409, `«${row.title}» уже встроен в NOAH и ставится вместе с приложением.`);
  db.prepare("UPDATE modules SET installs = installs + 1 WHERE id = ?").run(row.id);
  return { format: FORMAT, manifest: JSON.parse(row.manifest), files: JSON.parse(row.files) };
});

route("GET", /^\/api\/my\/modules$/, ({ user }) => {
  if (!user) throw new Fail(401, "Войдите.");
  return { modules: db.prepare("SELECT * FROM modules WHERE owner_id = ? ORDER BY updated DESC").all(user.id).map(card) };
});

route("DELETE", /^\/api\/my\/modules\/([a-z0-9-]+)$/, ({ user, match }) => {
  if (!user) throw new Fail(401, "Войдите.");
  const info = db.prepare("DELETE FROM modules WHERE id = ? AND owner_id = ?").run(match[1], user.id);
  if (!info.changes) throw new Fail(404, "Модуль не найден.");
  return { ok: true };
});

route("GET", /^\/api\/tokens$/, ({ user }) => {
  if (!user) throw new Fail(401, "Войдите.");
  return { tokens: db.prepare("SELECT id, label, created, used FROM tokens WHERE user_id = ? ORDER BY id DESC").all(user.id) };
});

route("POST", /^\/api\/tokens$/, async ({ req, user }) => {
  if (!user) throw new Fail(401, "Войдите.");
  const { label } = await readJson(req);
  const count = db.prepare("SELECT COUNT(*) AS n FROM tokens WHERE user_id = ?").get(user.id).n;
  if (count >= 10) throw new Fail(400, "Не больше 10 ключей — удалите ненужные.");
  const token = `noah_${randomBytes(24).toString("base64url")}`;
  db.prepare("INSERT INTO tokens (user_id, label, hash) VALUES (?, ?, ?)").run(
    user.id,
    String(label ?? "").trim().slice(0, 40) || "Ноа",
    sha(token),
  );
  return { token };
});

route("DELETE", /^\/api\/tokens\/(\d+)$/, ({ user, match }) => {
  if (!user) throw new Fail(401, "Войдите.");
  db.prepare("DELETE FROM tokens WHERE id = ? AND user_id = ?").run(Number(match[1]), user.id);
  return { ok: true };
});

/* ── Постоянная ссылка MCP ───────────────────────────────────────────────── */

// Одна ссылка на аккаунт, которую видно всегда. Раньше страница «Подключить
// ИИ» каждый раз выпускала новый ключ (прежний показать нельзя — хранится
// хеш), и нейросеть приходилось переподключать. Ключ этой ссылки хранится
// как есть: его видит только сам владелец, а сменить можно одной кнопкой.
db.exec(`
  CREATE TABLE IF NOT EXISTS mcp_links (
    user_id INTEGER PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    token_id INTEGER NOT NULL REFERENCES tokens(id) ON DELETE CASCADE,
    key TEXT NOT NULL
  );
`);

function issueMcpKey(userId) {
  const key = `noah_${randomBytes(24).toString("base64url")}`;
  const info = db.prepare("INSERT INTO tokens (user_id, label, hash) VALUES (?, ?, ?)").run(userId, "Ссылка MCP", sha(key));
  db.prepare("INSERT OR REPLACE INTO mcp_links (user_id, token_id, key) VALUES (?, ?, ?)").run(userId, Number(info.lastInsertRowid), key);
  return key;
}

route("GET", /^\/api\/my\/mcp$/, ({ user }) => {
  if (!user) throw new Fail(401, "Войдите.");
  const row = db.prepare("SELECT key, token_id FROM mcp_links WHERE user_id = ?").get(user.id);
  const key = row?.key ?? issueMcpKey(user.id);
  // Заходила ли нейросеть: по ссылке с ключом и по входу (OAuth). Без этого
  // коннектор со старой ссылкой молча получал отказ, и никто не видел.
  const linkUsed = row ? db.prepare("SELECT used FROM tokens WHERE id = ?").get(row.token_id)?.used ?? null : null;
  let clients = [];
  try {
    clients = db
      .prepare(
        `SELECT COALESCE(c.name, 'нейросеть') AS name, MAX(t.used) AS used FROM mcp_refresh r
         LEFT JOIN mcp_clients c ON c.id = r.client_id
         LEFT JOIN tokens t ON t.id = r.token_id
         WHERE r.user_id = ? GROUP BY r.client_id ORDER BY used DESC`,
      )
      .all(user.id);
  } catch {
    clients = [];
  }
  return { key, linkUsed, clients };
});

route("POST", /^\/api\/my\/mcp\/rotate$/, ({ user }) => {
  if (!user) throw new Fail(401, "Войдите.");
  const row = db.prepare("SELECT token_id FROM mcp_links WHERE user_id = ?").get(user.id);
  if (row) db.prepare("DELETE FROM tokens WHERE id = ? AND user_id = ?").run(row.token_id, user.id);
  return { key: issueMcpKey(user.id) };
});

/** Публикация из Ноа: модуль уже прошёл живую проверку у автора. */
/** Публикует модуль от имени автора. Общий путь для Ноа и для MCP по ссылке. */
function publishModule(user, { manifest, files, tools, description, category }) {
  const problems = lint(manifest, files);
  if (problems.length) throw new Fail(422, `Модуль не соответствует стандарту:\n${problems.map((p) => `  ✗ ${p}`).join("\n")}`);
  if (!Array.isArray(tools) || !tools.length || tools.some((t) => typeof t?.name !== "string")) {
    throw new Fail(422, "Нужен список инструментов из проверки Ноа.");
  }
  const existing = db.prepare("SELECT owner_id FROM modules WHERE id = ?").get(manifest.id);
  if (existing && existing.owner_id !== user.id) throw new Fail(409, `Имя «${manifest.id}» занято другим автором — выберите другой id.`);
  const cat = CATEGORIES.includes(category) ? category : "other";
  const cleanTools = tools.slice(0, 25).map((t) => ({ name: String(t.name).slice(0, 48), about: String(t.about ?? "").slice(0, 200) }));
  db.prepare(
    `INSERT INTO modules (id, owner_id, author, title, icon, about, voice, category, version, description, manifest, files, tools)
     VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
     ON CONFLICT(id) DO UPDATE SET title = excluded.title, icon = excluded.icon, about = excluded.about,
       voice = excluded.voice, category = excluded.category, version = excluded.version,
       description = excluded.description, manifest = excluded.manifest, files = excluded.files,
       tools = excluded.tools, updated = datetime('now')`,
  ).run(
    manifest.id,
    user.id,
    user.name,
    manifest.title,
    manifest.icon,
    manifest.about,
    manifest.voice,
    cat,
    String(manifest.version || "1.0.0").slice(0, 20),
    String(description ?? "").slice(0, 2000),
    JSON.stringify(manifest),
    JSON.stringify(files ?? {}),
    JSON.stringify(cleanTools),
  );
  return { ok: true, id: manifest.id, url: `/#/module/${manifest.id}` };
}

route("POST", /^\/api\/publish$/, async ({ req, tokenUser: user }) => {
  if (!user) throw new Fail(401, "Нужен ключ площадки: создайте его в кабинете и впишите в Ноа.");
  return publishModule(user, await readJson(req));
});

route("GET", /^\/api\/whoami$/, ({ tokenUser: user }) => {
  if (!user) throw new Fail(401, "Ключ не подошёл.");
  return { name: user.name };
});

/* ── Статика ────────────────────────────────────────────────────────────── */

const TYPES = {
  ".html": "text/html; charset=utf-8",
  ".js": "text/javascript; charset=utf-8",
  ".css": "text/css; charset=utf-8",
  ".svg": "image/svg+xml",
  ".png": "image/png",
  ".webp": "image/webp",
  ".ico": "image/x-icon",
  ".json": "application/json; charset=utf-8",
  ".webmanifest": "application/manifest+json; charset=utf-8",
  ".woff2": "font/woff2",
  ".woff": "font/woff",
  ".ttf": "font/ttf",
};

/**
 * Метка версии сайта — отпечаток всех его файлов.
 *
 * Файлы сайта подключаются с меткой (`core.js?v=54`) и кэшируются браузером на
 * год. Метку поднимали руками и забывали: правка лежала на сервере, а
 * вернувшийся человек неделями видел старое. Теперь сервер сам подставляет
 * вместо любого `?v=<число>` отпечаток: изменился хоть один файл — у всех
 * ссылок новая метка. Пересчитывается не чаще раза в пять секунд: файлы
 * обновляются копированием, без перезапуска.
 */
const stampOf = (dir) => {
  let value = "";
  let at = 0;
  return () => {
    if (value && Date.now() - at < 5000) return value;
    const hash = createHash("sha1");
    for (const entry of readdirSync(dir, { recursive: true, withFileTypes: true })) {
      if (!entry.isFile()) continue;
      const path = join(entry.parentPath ?? entry.path, entry.name);
      const info = statSync(path);
      hash.update(`${path}:${info.size}:${info.mtimeMs};`);
    }
    value = hash.digest("hex").slice(0, 10);
    at = Date.now();
    return value;
  };
};
const siteStamp = stampOf(WEB);

/**
 * Метка версии Ноа онлайн (src/): у её файлов метки в адресах нет, модули
 * тянут друг друга по голым адресам. CDN зеркала отдавал старые копии из
 * своей памяти, не спрашивая сервер, — и страница жила на вчерашнем коде.
 * Сервер сам дописывает `?v=<метка>` к подключаемым файлам в страницах и к
 * импортам в модулях: изменился хоть один файл — у всех адресов новая метка.
 */
const appStamp = stampOf(APP);
const withVersion = (text, stamp) =>
  text
    .replace(/((?:src|href)=")(\.\/[\w./-]+\.(?:js|css))"/g, `$1$2?v=${stamp}"`)
    .replace(/((?:\bfrom\s*|\bimport\s*\(\s*|\bimport\s+)(["']))(\.{1,2}\/[\w./-]+\.js)\2/g, `$1$3?v=${stamp}$2`);

const STAMPED = new Set([".html", ".js", ".css", ".webmanifest"]);

async function serveStatic(req, res, pathname, versioned) {
  let decoded;
  try {
    decoded = decodeURIComponent(pathname);
  } catch {
    return false;
  }
  let headers = SECURITY_HEADERS;
  let file;
  if (decoded === "/app" || decoded.startsWith("/app/")) {
    // Ноа онлайн: только перечисленное, без подстановки главной сайта.
    const inner = decoded === "/app" || decoded === "/app/" ? "/app.html" : decoded.slice(4);
    if (!APP_FILES.test(inner) || inner.includes("..")) return false;
    headers = { ...SECURITY_HEADERS, "Content-Security-Policy": APP_CSP };
    file = normalize(join(APP, inner));
    if (!file.startsWith(APP + sep)) return false;
    try {
      if (!(await stat(file)).isFile()) return false;
    } catch {
      return false;
    }
  } else {
    file = normalize(join(WEB, decoded));
    // Именно папка сайта с разделителем: иначе соседняя «web-old» прошла бы проверку.
    if (file !== WEB && !file.startsWith(WEB + sep)) return false;
    try {
      if ((await stat(file)).isDirectory()) file = join(file, "index.html");
    } catch {
      file = join(WEB, "index.html");
    }
  }
  try {
    const type = TYPES[extname(file)] ?? "application/octet-stream";
    const info = await stat(file);
    // Файлы с версией в адресе (?v=) не меняются — их браузер берёт из кэша.
    // Остальное сверяется каждый раз, чтобы правки были видны сразу.
    const cacheControl = versioned
      ? "public, max-age=31536000, immutable"
      : [".svg", ".png", ".webp", ".ico", ".woff2", ".woff", ".ttf"].includes(extname(file))
        ? "public, max-age=86400"
        : "no-cache";
    // Сверка — по отпечатку файла: не изменился — «304», без тела. Раньше
    // каждый заход заново тянул все скрипты и стили, и по мобильной сети
    // Ноа онлайн открывалась по нескольку секунд.
    const inApp = file.startsWith(APP + sep) && [".html", ".js"].includes(extname(file));
    const stamp = inApp ? appStamp() : file.startsWith(WEB + sep) && STAMPED.has(extname(file)) ? siteStamp() : "";
    const etag = `W/"${info.size.toString(16)}-${Math.trunc(info.mtimeMs).toString(16)}${stamp && `-${stamp}`}"`;
    if (req.headers["if-none-match"] === etag) {
      res.writeHead(304, { ...headers, ETag: etag, "Cache-Control": cacheControl });
      res.end();
      return true;
    }
    let raw = await readFile(file);
    if (inApp) raw = Buffer.from(withVersion(raw.toString("utf8"), stamp));
    else if (stamp) raw = Buffer.from(raw.toString("utf8").replace(/\?v=\d+/g, `?v=${stamp}`));
    const key = `${file}:${info.mtimeMs}:${stamp}`;
    const out = /^(text|application\/(json|manifest)|image\/svg)/.test(type) ? packed(req, raw, key) : { body: raw, headers: {} };
    res.writeHead(200, {
      ...headers,
      "Content-Type": type,
      "Content-Length": out.body.length,
      ETag: etag,
      "Cache-Control": cacheControl,
      ...out.headers,
    });
    res.end(req.method === "HEAD" ? undefined : out.body);
    return true;
  } catch {
    return false;
  }
}

/* ── Скачать NOAH: сразу установщик последнего релиза ───────────────────── */

const RELEASES = "https://github.com/faafaafuu/asis/releases/latest";
/**
 * Файлы последнего релиза по системам: Windows, macOS, Linux, Android.
 *
 * Сайт показывает кнопку только той системы, чей файл в релизе есть, — вести
 * на несуществующий файл хуже, чем не предлагать систему вовсе.
 */
const PLATFORMS = {
  windows: [/setup\.exe$/i, /\.(exe|msi)$/i],
  mac: [/\.dmg$/i],
  linux: [/\.AppImage$/i, /\.deb$/i],
  android: [/\.apk$/i],
};
let release = { files: {}, version: "", at: 0 };

async function latestRelease() {
  if (release.at && Date.now() - release.at < 10 * 60_000) return release;
  try {
    const response = await fetch("https://api.github.com/repos/faafaafuu/asis/releases/latest", {
      headers: { Accept: "application/vnd.github+json", "User-Agent": "noah-platform" },
      signal: AbortSignal.timeout(8000),
    });
    const body = await response.json();
    const assets = body.assets ?? [];
    const files = {};
    for (const [os, patterns] of Object.entries(PLATFORMS)) {
      for (const pattern of patterns) {
        const asset = assets.find((a) => pattern.test(a.name) && !/\.sig$/i.test(a.name));
        if (asset) {
          files[os] = { url: asset.browser_download_url, name: asset.name, size: asset.size };
          break;
        }
      }
    }
    release = { files, version: String(body.tag_name ?? "").replace(/^v/, ""), at: Date.now() };
  } catch (err) {
    console.error("последний релиз не получен:", err.message);
  }
  return release;
}

/** Система по браузеру: для кнопки «Скачать» без выбора. */
function osOf(agent) {
  if (/android/i.test(agent)) return "android";
  if (/mac os x|macintosh/i.test(agent) && !/iphone|ipad/i.test(agent)) return "mac";
  if (/linux|x11/i.test(agent)) return "linux";
  return "windows";
}

async function latestInstaller(os) {
  const { files } = await latestRelease();
  return files[os]?.url ?? files.windows?.url ?? RELEASES;
}

route("GET", /^\/api\/downloads$/, async () => {
  const { files, version } = await latestRelease();
  return { version, files };
});

const oauthHandler = mountOAuth({ route, db, Fail, readJson, sessionUser, openSession, cookies, cookieDomain, mirrorOrigin });
const PUBLIC_URL = (process.env.NOAH_PUBLIC_URL ?? `http://127.0.0.1:${PORT}`).replace(/\/$/, "");
const mcpAuthHandler = mountMcpAuth({ db, publicUrl: PUBLIC_URL, sessionUser });
mountNoa({ route, db, Fail, readJson });
const mcpHandler = mountRemote({ route, db, Fail, readJson, userForKey, publishModule, lint, validId, send, maxBody: MAX_BODY, publicUrl: PUBLIC_URL });

async function handle(req, res) {
  // Перед сайтом может стоять CDN: всё, что сервер не разрешил кэшировать
  // явно, не кэшируется. Иначе перенаправление после входа — с cookie
  // сессии — CDN мог бы отдать следующему человеку. Статика задаёт своё
  // правило в writeHead, и оно заменяет это.
  res.setHeader("Cache-Control", "no-store");
  if (COOKIE_DOMAIN && !cookieFits(req)) dropCookieDomain(res);
  const url = new URL(req.url, "http://local");
  // Заголовку прокси верим, только если запрос пришёл от него самого.
  const peer = String(req.socket.remoteAddress ?? "");
  // Свои: сам хост и сеть Docker, где стоит nginx с HTTPS для домена.
  const local = peer === "127.0.0.1" || peer === "::1" || /^(::ffff:)?(127\.|172\.(1[6-9]|2\d|3[01])\.)/.test(peer);
  const ip = local && req.headers["x-real-ip"] ? String(req.headers["x-real-ip"]) : peer;
  // По пути к серверу из некоторых сетей соединение замирает, когда через него
  // прошло около 30 КБ. Каждый ответ — в своём соединении, и замирать нечему.
  if (!local) res.shouldKeepAlive = false;
  try {
    // MCP по ссылке: нейросеть пользователя подключается сюда адресом с ключом.
    if (url.pathname === "/mcp") return await mcpHandler(req, res, url);
    // Вход нейросети в MCP по OAuth: описание сервера, регистрация, согласие, токены.
    if ((url.pathname.startsWith("/.well-known/") || url.pathname.startsWith("/oauth/")) && (await mcpAuthHandler(req, res, url))) return;
    // Страницы по голому IP — на российское зеркало. Прямой путь к серверу
    // из РФ замирает после ~16 КБ на соединение: шрифты, модули и список
    // курсов застревали, забивали все соединения браузера, и даже «кто
    // вошёл» не доходил — страница писала «Нет связи». Через CDN зеркала всё
    // приходит целиком. API по IP продолжает отвечать как раньше.
    const pageOnIp =
      req.method === "GET" &&
      /^[\d.]+(:\d+)?$/.test(String(req.headers.host ?? "")) &&
      (url.pathname === "/" || url.pathname === "/app" || url.pathname === "/app/" || /^\/app\/[\w-]+\.html$/.test(url.pathname));
    const mirrorHost = [...MIRRORS].find((host) => !/^[\d.]+(:\d+)?$/.test(host));
    if (pageOnIp && mirrorHost) {
      const search = url.pathname.startsWith("/app") && !url.search ? `?v=${appStamp()}` : url.search;
      res.writeHead(302, { Location: `https://${mirrorHost}${url.pathname}${search}`, "Cache-Control": "no-store" });
      return res.end();
    }
    // Голый /app/ — на адрес с меткой версии: сам /app/ CDN зеркала держит
    // в памяти и отдавал вчерашнюю страницу со вчерашним кодом.
    if (req.method === "GET" && (url.pathname === "/app" || url.pathname === "/app/") && !url.search) {
      res.writeHead(302, { Location: `/app/?v=${appStamp()}`, "Cache-Control": "no-store" });
      return res.end();
    }
    if (url.pathname === "/download") {
      const asked = url.searchParams.get("os") ?? osOf(String(req.headers["user-agent"] ?? ""));
      res.writeHead(302, { Location: await latestInstaller(asked), "Cache-Control": "no-store" });
      return res.end();
    }
    if (url.pathname.startsWith("/auth/") && (await oauthHandler(req, res, url))) return;
    if (url.pathname.startsWith("/api/tunnel/") && req.method === "GET") {
      return await tunnel({ req, res, url, cookies, handle, Fail, maxBody: MAX_BODY });
    }
    if (url.pathname.startsWith("/api/")) {
      if (req.method !== "GET" && !sameOrigin(req)) throw new Fail(403, "Чужой источник запроса.");
      for (const r of routes) {
        const match = r.pattern.exec(url.pathname);
        if (!match || r.method !== req.method) continue;
        const body = await r.handler({ req, res, url, match, ip, user: sessionUser(req), tokenUser: tokenUser(req) });
        // Обработчик ответил сам — например, звуком, а не JSON.
        if (res.headersSent) return;
        return send(res, 200, body);
      }
      throw new Fail(404, "Нет такого адреса.");
    }
    if (req.method !== "GET" && req.method !== "HEAD") throw new Fail(405, "Метод не поддерживается.");
    if (!(await serveStatic(req, res, url.pathname, url.searchParams.has("v")))) throw new Fail(404, "Не найдено.");
  } catch (err) {
    const status = err instanceof Fail ? err.status : 500;
    if (status === 500) console.error(err);
    if (!res.headersSent) send(res, status, { error: status === 500 ? "Ошибка сервера." : err.message });
    else res.end();
  }
}

/*
 * Вход по IP. Из мобильного интернета РФ сайт по домену не открывается, а по
 * голому IP — открывается. Чтобы по IP работали вход, микрофон и cookie,
 * нужен HTTPS, а порт один: сервер по первому байту соединения понимает,
 * TLS это или HTTP, и отдаёт его нужному обработчику. Сертификат на IP
 * выпускается на 6 дней и продлевается сам — сервер перечитывает его раз в час.
 */
const TLS_CERT = process.env.NOAH_TLS_CERT ?? "";
const TLS_KEY = process.env.NOAH_TLS_KEY ?? "";

function tlsContext() {
  return tls.createSecureContext({ cert: readFileSync(TLS_CERT), key: readFileSync(TLS_KEY) });
}

let secureContext = null;
if (TLS_CERT && TLS_KEY) {
  try {
    secureContext = tlsContext();
    setInterval(() => {
      try {
        secureContext = tlsContext();
      } catch (err) {
        console.error("Сертификат не перечитался:", err.message);
      }
    }, 3600e3).unref();
  } catch (err) {
    console.error("HTTPS выключен — сертификат не читается:", err.message);
  }
}

const plain = http.createServer(handle);

/**
 * TLS поверх уже начатого соединения. Первый байт прочитан, чтобы узнать
 * протокол, и TLS-обработчику Node его уже не увидеть: он читает сокет
 * напрямую. Поэтому соединение идёт через промежуточный поток, куда
 * прочитанное возвращено, а адрес клиента переносится на TLS-сокет.
 */
function secured(socket, first) {
  const relay = new Duplex({
    read: () => socket.resume(),
    write: (chunk, encoding, done) => socket.write(chunk, encoding, done),
    final: (done) => {
      socket.end();
      done();
    },
    destroy: (err, done) => {
      socket.destroy();
      done(err);
    },
  });
  relay.push(first);
  socket.on("data", (chunk) => relay.push(chunk) || socket.pause());
  socket.on("end", () => relay.push(null));
  socket.on("close", () => relay.destroy());
  const tlsSocket = new tls.TLSSocket(relay, { isServer: true, secureContext, ALPNProtocols: ["http/1.1"] });
  Object.defineProperty(tlsSocket, "remoteAddress", { value: socket.remoteAddress });
  tlsSocket.on("error", () => tlsSocket.destroy());
  return tlsSocket;
}

const server = net.createServer((socket) => {
  socket.on("error", () => socket.destroy());
  socket.once("data", (first) => {
    socket.pause();
    // 0x16 — начало TLS-рукопожатия.
    if (secureContext && first[0] === 0x16) return plain.emit("connection", secured(socket, first));
    socket.unshift(first);
    plain.emit("connection", socket);
    socket.resume();
  });
});

setInterval(() => db.prepare("DELETE FROM sessions WHERE expires < ?").run(new Date().toISOString()), 3600e3).unref();

server.listen(PORT, HOST, () => console.log(`NOAH platform on ${HOST}:${PORT}${secureContext ? " (HTTP и HTTPS)" : ""}`));
