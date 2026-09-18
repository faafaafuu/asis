// Проверка площадки по HTTP: node selftest.mjs http://127.0.0.1:8795
// Заводит временный аккаунт, публикует модуль, ставит его и всё убирает за собой.

const BASE = process.argv[2] ?? "http://127.0.0.1:8795";
const stamp = Date.now().toString(36);
let cookie = "";
let failed = 0;

async function call(path, { method = "GET", body, headers = {} } = {}) {
  const response = await fetch(BASE + path, {
    method,
    headers: { ...(body ? { "Content-Type": "application/json" } : {}), ...(cookie ? { Cookie: cookie } : {}), ...headers },
    body: body ? JSON.stringify(body) : undefined,
  });
  const set = response.headers.get("set-cookie");
  if (set) cookie = set.split(";")[0];
  return { status: response.status, data: await response.json().catch(() => ({})) };
}

function expect(name, ok, detail = "") {
  console.log(`${ok ? "✓" : "✗"} ${name}${ok ? "" : ` — ${JSON.stringify(detail)}`}`);
  if (!ok) failed++;
}

const email = `selftest-${stamp}@example.com`;
const name = `selftest-${stamp}`;

let r = await call("/api/auth/register", { method: "POST", body: { email, name, password: "short" } });
expect("короткий пароль отклонён", r.status === 400, r);

r = await call("/api/auth/register", { method: "POST", body: { email, name, password: "correct-horse-battery" } });
expect("регистрация", r.status === 200 && r.data.user?.name === name, r);

r = await call("/api/me");
expect("сессия после регистрации", r.data.user?.email === email, r);

r = await call("/api/tokens", { method: "POST", body: { label: "selftest" }, headers: { Origin: "http://evil.example" } });
expect("чужой источник отклонён", r.status === 403, r);

r = await call("/api/tokens", { method: "POST", body: { label: "selftest" } });
const token = r.data.token;
expect("ключ площадки создан", typeof token === "string" && token.startsWith("noah_"), r);

const manifest = {
  id: `selftest-${stamp}`,
  title: "Проверка",
  icon: "✓",
  about: "Модуль самопроверки площадки",
  voice: "«Ноа, проверь площадку»",
  version: "1.0.0",
  mcp: { command: "node", args: ["%MODULE_DIR%\\server.mjs"] },
  tests: [{ tool: "ping", args: {}, expect: "ок" }],
};
const files = { "server.mjs": "// сервер\n" };
const tools = [{ name: "ping", about: "отвечает «ок»" }];

const saved = cookie;
cookie = "";
r = await call("/api/publish", { method: "POST", body: { manifest, files, tools } });
expect("публикация без ключа отклонена", r.status === 401, r);

r = await call("/api/publish", {
  method: "POST",
  body: { manifest: { ...manifest, mcp: { command: "cmd", args: ["& calc"] } }, files, tools },
  headers: { Authorization: `Bearer ${token}` },
});
expect("нарушение стандарта отклонено", r.status === 422 && /cmd/.test(r.data.error), r);

r = await call("/api/publish", {
  method: "POST",
  body: { manifest, files: { ...files, "evil.bat": "x" }, tools },
  headers: { Authorization: `Bearer ${token}` },
});
expect("исполняемый файл отклонён", r.status === 422, r);

r = await call("/api/publish", { method: "POST", body: { manifest, files, tools, category: "dev" }, headers: { Authorization: `Bearer ${token}` } });
expect("публикация по ключу", r.status === 200 && r.data.id === manifest.id, r);

r = await call(`/api/modules?q=${encodeURIComponent("самопроверки")}`);
expect("модуль в поиске", r.data.modules?.some((m) => m.id === manifest.id), r);

r = await call(`/api/modules/${manifest.id}`);
expect("страница модуля", r.data.tools?.[0]?.name === "ping" && r.data.author === name, r);

r = await call(`/api/modules/${manifest.id}/package`);
expect("пакет для установки", r.data.manifest?.id === manifest.id && r.data.files?.["server.mjs"], r);

r = await call(`/api/modules/${manifest.id}`);
expect("счётчик установок", r.data.installs === 1, r);

cookie = saved;
r = await call("/api/my/modules");
expect("мои модули", r.data.modules?.length === 1, r);

r = await call(`/api/my/modules/${manifest.id}`, { method: "DELETE" });
expect("модуль удалён", r.status === 200, r);

r = await call("/api/auth/logout", { method: "POST" });
r = await call("/api/me");
expect("выход", r.data.user === null, r);

r = await call("/api/auth/login", { method: "POST", body: { email, password: "wrong-password-123" } });
expect("неверный пароль", r.status === 401, r);

r = await call("/api/auth/login", { method: "POST", body: { email, password: "correct-horse-battery" } });
expect("вход", r.status === 200, r);

r = await call("/api/account", { method: "DELETE", body: { password: "correct-horse-battery" } });
expect("аккаунт удалён", r.status === 200, r);

console.log(failed ? `\nОшибок: ${failed}` : "\nВсё прошло.");
process.exit(failed ? 1 : 0);
