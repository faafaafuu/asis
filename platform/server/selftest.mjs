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

// Курс по ссылке MCP: нейросеть отправляет, «Ноа» забирает кусками и отвечает.
const mcp = (id, name, args) =>
  call(`/mcp?key=${token}`, { method: "POST", body: { jsonrpc: "2.0", id, method: "tools/call", params: { name, arguments: args } } });
const app = { headers: { Authorization: `Bearer ${token}` } };
r = await call(`/mcp?key=${token}`, { method: "POST", body: { jsonrpc: "2.0", id: 1, method: "tools/list" } });
expect("инструменты курсов", ["course_format", "create_course", "add_topic", "list_courses", "course_status"].every((n) => r.data.result?.tools?.some((t) => t.name === n)), r);
r = await mcp(2, "course_format", {});
expect("формат курса", /concepts/.test(r.data.result?.content?.[0]?.text ?? ""), r);
r = await call("/api/app/hello", { method: "POST", body: { env: "selftest", courses: "Курсов нет." }, ...app });
expect("Ноа на связи", r.status === 200, r);
// Урок длиннее куска (6000 знаков), с эмодзи — куски режутся по UTF-16.
const lesson = ["## Первый", "## Второй", "## Третий"].map((head) => `${head}\n\n${"Урок — ".repeat(1000)}`).join("\n\n") + " ёж 🦔";
/** Курс, который проходит проверку формата: три раздела, шесть понятий, задача и вопрос. */
const validCourse = (id, title) => ({
  id,
  title,
  topics: [
    {
      id: "t",
      title: "Тема",
      lesson,
      concepts: Array.from({ length: 6 }, (_, i) => ({ id: `c${i}`, term: `Понятие ${i}`, definition: `Определение понятия ${i}.` })),
      tasks: [{ id: "t-task", kind: "open", q: "Объясните понятие 0.", points: ["что это"], reference: "Это понятие 0." }],
      exam: [{ id: "t-exam", kind: "choice", q: "Что такое понятие 0?", options: ["Определение 0", "Другое"], answer: 0 }],
    },
  ],
});
const sent = mcp(3, "create_course", { course: validCourse(`selftest-${stamp}`, "Проверка") });
let jobs = [];
for (let i = 0; i < 20 && !jobs.length; i++) {
  await new Promise((resolve) => setTimeout(resolve, 300));
  jobs = (await call("/api/app/course-jobs", app)).data.jobs ?? [];
}
expect("курс в очереди", jobs.length === 1 && jobs[0].kind === "course", jobs);
let text = "";
let size = 1;
while (jobs[0] && text.length < size) {
  r = await call(`/api/app/course-jobs/${jobs[0].id}/part?at=${text.length}`, app);
  size = r.data.size;
  if (!r.data.text) break;
  text += r.data.text;
}
expect("курс собран из кусков", JSON.parse(text || "{}").topics?.[0]?.lesson === lesson, text.length);
// Курсы аккаунта — устройству напрямую, мимо очереди: список и куски.
r = await call("/api/app/courses", app);
const own = r.data.courses?.find((c) => c.id === `selftest-${stamp}`);
expect("курс в списке аккаунта", own && own.size > 0, r.data);
text = "";
size = 1;
while (own && text.length < size) {
  r = await call(`/api/app/courses/${own.id}/part?at=${text.length}`, app);
  size = r.data.size;
  if (!r.data.text) break;
  text += r.data.text;
}
expect("курс аккаунта собран из кусков", JSON.parse(text || "{}").topics?.[0]?.lesson === lesson, text.length);
// Курс с компьютера — в аккаунт кусками.
const upload = JSON.stringify(validCourse(`up-${stamp}`, "Выгрузка"));
let reply = null;
for (let at = 0; at < upload.length; at += 5000) {
  r = await call(`/api/app/courses/up-${stamp}/part`, { method: "POST", body: { at, text: upload.slice(at, at + 5000), size: upload.length }, ...app });
  reply = r;
  if (r.status !== 200) break;
}
expect("курс выгружен в аккаунт", reply?.status === 200 && /сохранён/.test(reply.data.report ?? ""), reply);
r = await call(`/api/app/courses/up-${stamp}/part`, { method: "POST", body: { at: 5000, text: "x", size: upload.length }, ...app });
expect("кусок не по порядку отклонён", r.status === 409, r);
r = await call("/api/app/courses", app);
expect("выгруженный курс в списке", r.data.courses?.some((c) => c.id === `up-${stamp}`), r.data);
r = await call(`/api/app/course-jobs/${jobs[0]?.id}/report`, { method: "POST", body: { ok: true, report: "Курс сохранён." }, ...app });
r = await sent;
expect("отчёт дошёл до нейросети", /Курс сохранён\.$/.test(r.data.result?.content?.[0]?.text ?? ""), r);
r = await mcp(4, "course_status", { id: `selftest-${stamp}` });
expect("статус курса", /принят/.test(r.data.result?.content?.[0]?.text ?? ""), r);
r = await mcp(5, "list_courses", {});
expect("список курсов", /Курсов нет/.test(r.data.result?.content?.[0]?.text ?? ""), r);

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
