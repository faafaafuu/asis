// Запись через зеркало, которое пропускает только чтение.
//
// CDN перед зеркалом (m.noahlab.ru) пропускает к серверу лишь GET и HEAD,
// а на POST, PUT и DELETE сам отвечает «405». Поэтому с зеркала страница
// шлёт запись как GET: тело — кусками в адресе, затем команда «выполнить».
// Сервер собирает тело и проводит запрос через те же маршруты, что и
// обычный POST: для обработчиков разницы нет.
//
// Чужой сайт так не сделает: в каждом запросе метка, которую страница сама
// кладёт в cookie (SameSite=Strict) и повторяет в адресе. Чужая страница
// не может ни прочесть эту cookie, ни поставить свою.

import { Readable } from "node:stream";

const METHODS = new Set(["POST", "PUT", "PATCH", "DELETE"]);
const LIFETIME = 5 * 60_000;
const MAX_PENDING = 500;

/** Недособранные тела: id → { parts, size, at }. */
const pending = new Map();

setInterval(() => {
  const old = Date.now() - LIFETIME;
  for (const [id, entry] of pending) if (entry.at < old) pending.delete(id);
}, 60_000).unref();

const decode = (text) => Buffer.from(String(text ?? ""), "base64url");

/**
 * Обрабатывает /api/tunnel/part и /api/tunnel/go. `handle` — общий
 * обработчик сервера: ему отдаётся собранный запрос.
 */
export async function tunnel({ req, res, url, cookies, handle, Fail, maxBody }) {
  const mark = cookies(req).noah_t;
  if (!mark || mark.length < 16 || url.searchParams.get("k") !== mark) throw new Fail(403, "Чужой источник запроса.");
  if (req.headers["sec-fetch-site"] === "cross-site") throw new Fail(403, "Чужой источник запроса.");
  const id = String(url.searchParams.get("id") ?? "");
  if (!/^[\w-]{8,64}$/.test(id)) throw new Fail(400, "Запрос не разобрался.");

  if (url.pathname === "/api/tunnel/part") {
    const index = Number(url.searchParams.get("i"));
    if (!Number.isInteger(index) || index < 0 || index > 10_000) throw new Fail(400, "Запрос не разобрался.");
    let entry = pending.get(id);
    if (!entry) {
      if (pending.size >= MAX_PENDING) throw new Fail(503, "Сервер занят — попробуйте через минуту.");
      entry = { parts: [], size: 0, at: Date.now(), mark };
      pending.set(id, entry);
    }
    if (entry.mark !== mark) throw new Fail(403, "Чужой источник запроса.");
    const part = decode(url.searchParams.get("d"));
    entry.size += part.length;
    if (entry.size > maxBody) {
      pending.delete(id);
      throw new Fail(413, "Слишком большой запрос.");
    }
    entry.parts[index] = part;
    res.writeHead(204, { "Cache-Control": "no-store" });
    return res.end();
  }

  if (url.pathname !== "/api/tunnel/go") throw new Fail(404, "Нет такого адреса.");
  const method = String(url.searchParams.get("m") ?? "").toUpperCase();
  const path = String(url.searchParams.get("p") ?? "");
  if (!METHODS.has(method)) throw new Fail(400, "Запрос не разобрался.");
  if (!/^\/(api|auth)\//.test(path) || path.startsWith("/api/tunnel/")) throw new Fail(400, "Запрос не разобрался.");

  let body;
  const count = Number(url.searchParams.get("n") ?? 0);
  if (count > 0) {
    const entry = pending.get(id);
    pending.delete(id);
    if (!entry || entry.mark !== mark) throw new Fail(400, "Запрос дошёл не целиком — повторите.");
    const parts = entry.parts.slice(0, count);
    if (parts.length !== count || parts.includes(undefined) || entry.parts.length !== count) {
      throw new Fail(400, "Запрос дошёл не целиком — повторите.");
    }
    body = Buffer.concat(parts);
  } else {
    body = decode(url.searchParams.get("d"));
    if (body.length > maxBody) throw new Fail(413, "Слишком большой запрос.");
  }

  // Собранный запрос: тот же сокет и cookie, свой метод, адрес и тело.
  const inner = Readable.from(body.length ? [body] : []);
  inner.method = method;
  inner.url = path;
  inner.socket = req.socket;
  inner.headers = { ...req.headers, "content-length": String(body.length) };
  const type = url.searchParams.get("t");
  if (type) inner.headers["content-type"] = type;
  else delete inner.headers["content-type"];
  return handle(inner, res);
}
