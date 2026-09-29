// Туннель записи через зеркало: быстрый ответ сразу, долгий — по частям
// ожидания, повтор от CDN не запускает работу второй раз.

import { test, before, after } from "node:test";
import assert from "node:assert/strict";
import http from "node:http";
import { tunnel } from "./tunnel.mjs";

class Fail extends Error {
  constructor(status, message) {
    super(message);
    this.status = status;
  }
}

const cookies = (req) =>
  Object.fromEntries(
    String(req.headers.cookie ?? "")
      .split(";")
      .map((pair) => pair.trim().split("="))
      .filter(([name]) => name),
  );

const MARK = "abcdefabcdefabcdef";
let calls = {};
let server;
let base;

async function readBody(req) {
  const chunks = [];
  for await (const chunk of req) chunks.push(chunk);
  return Buffer.concat(chunks).toString("utf8");
}

/** Маршруты-заглушки: быстрый, долгий и с cookie. */
async function handle(req, res) {
  const url = new URL(req.url, "http://local");
  try {
    if (url.pathname.startsWith("/api/tunnel/")) {
      return await tunnel({ req, res, url, cookies, handle, Fail, maxBody: 1 << 20, wait: 150 });
    }
    calls[url.pathname] = (calls[url.pathname] ?? 0) + 1;
    const body = await readBody(req);
    if (url.pathname === "/api/slow") await new Promise((resolve) => setTimeout(resolve, 600));
    // Как send() на сервере: заголовки запроса берутся из res.req.
    assert.ok(res.req?.headers, "у ответа есть запрос");
    res.writeHead(200, { "Content-Type": "application/json", "Set-Cookie": "noah_session=s1; Path=/" });
    res.end(JSON.stringify({ method: req.method, type: req.headers["content-type"] ?? null, body }));
  } catch (err) {
    res.writeHead(err.status ?? 500, { "Content-Type": "application/json" });
    res.end(JSON.stringify({ error: err.message }));
  }
}

before(async () => {
  server = http.createServer(handle);
  await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
  base = `http://127.0.0.1:${server.address().port}`;
});

after(() => server.close());

const get = (path, mark = MARK) => fetch(`${base}${path}`, { headers: mark ? { Cookie: `noah_t=${mark}` } : {} });
const query = (params) => new URLSearchParams({ k: MARK, ...params });
const b64 = (text) => Buffer.from(text).toString("base64url");

test("быстрая запись отвечает сразу, с cookie и телом", async () => {
  calls = {};
  const response = await get(`/api/tunnel/go?${query({ id: "fast-0001", m: "POST", p: "/api/fast", t: "application/json", d: b64('{"a":1}') })}`);
  assert.equal(response.status, 200);
  assert.match(response.headers.get("set-cookie"), /noah_session=s1/);
  assert.deepEqual(await response.json(), { method: "POST", type: "application/json", body: '{"a":1}' });
  assert.equal(calls["/api/fast"], 1);
});

test("долгая работа: «ещё работаю», потом ответ; повтор не запускает её снова", async () => {
  calls = {};
  const go = `/api/tunnel/go?${query({ id: "slow-0001", m: "POST", p: "/api/slow", d: b64("x") })}`;
  const first = await get(go);
  assert.equal(first.status, 202);
  assert.equal(first.headers.get("x-noah-pending"), "1");
  assert.deepEqual(await first.json(), { tunnel: "pending" });

  // Повтор той же команды — так делает CDN, когда устал ждать.
  const again = await get(go);
  assert.equal(again.status, 202);
  await again.body?.cancel();

  let response;
  for (let i = 0; i < 20; i++) {
    response = await get(`/api/tunnel/wait?${query({ id: "slow-0001" })}`);
    if (response.status !== 202) break;
    await response.body?.cancel();
  }
  assert.equal(response.status, 200);
  assert.equal((await response.json()).method, "POST");
  assert.equal(calls["/api/slow"], 1, "работа выполнена один раз");

  // Готовый ответ можно забрать ещё раз: первый мог потеряться по дороге.
  const repeat = await get(`/api/tunnel/wait?${query({ id: "slow-0001" })}`);
  assert.equal(repeat.status, 200);
  await repeat.body?.cancel();
});

test("тело кусками собирается в исходное", async () => {
  calls = {};
  const text = "ы".repeat(5000);
  const bytes = Buffer.from(text);
  const size = 3000;
  const count = Math.ceil(bytes.length / size);
  for (let i = 0; i < count; i++) {
    const part = await get(`/api/tunnel/part?${query({ id: "parts-001", i: String(i), d: bytes.subarray(i * size, (i + 1) * size).toString("base64url") })}`);
    assert.equal(part.status, 204);
  }
  const response = await get(`/api/tunnel/go?${query({ id: "parts-001", m: "PUT", p: "/api/fast", t: "text/plain", n: String(count) })}`);
  assert.equal(response.status, 200);
  const data = await response.json();
  assert.equal(data.method, "PUT");
  assert.equal(data.body, text);
});

test("без метки в cookie — отказ", async () => {
  const response = await get(`/api/tunnel/go?${query({ id: "nomark-01", m: "POST", p: "/api/fast", d: "" })}`, null);
  assert.equal(response.status, 403);
});

test("неизвестная работа — «ответ не сохранился»", async () => {
  const response = await get(`/api/tunnel/wait?${query({ id: "nothing-01" })}`);
  assert.equal(response.status, 410);
});

test("туннель не ведёт в сам себя и не принимает GET", async () => {
  const self = await get(`/api/tunnel/go?${query({ id: "self-0001", m: "POST", p: "/api/tunnel/go", d: "" })}`);
  assert.equal(self.status, 400);
  const read = await get(`/api/tunnel/go?${query({ id: "read-0001", m: "GET", p: "/api/fast", d: "" })}`);
  assert.equal(read.status, 400);
});
