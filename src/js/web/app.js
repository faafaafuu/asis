// Главная Ноа в браузере: разговор, своя модель и курсы обучения.
//
// Всё, что в программе делает Rust, здесь делает браузер: модель зовётся
// прямо со страницы (ai-web.js), курсы и прогресс — noa-store.js. Сайт
// видит только курсы и прогресс вошедшего человека, но не его ключ.

import { PROVIDERS, loadModel, saveModel, listModels, chat, bridgeAvailable } from "./ai-web.js";
import { openNoa } from "./noa-store.js";
import { mountDictionary } from "./web-api.js";
import { speak as sayAloud, stopSpeaking, canListen } from "./voice.js";
import { startTalk, orbIcon } from "./talk.js";
import { buildCourse, Stopped } from "./course-builder.js";

const ui = {};
for (const node of document.querySelectorAll("[data-el]")) ui[node.dataset.el] = node;

const theme = (() => {
  try {
    return localStorage.getItem("noa.theme");
  } catch {
    return null;
  }
})();
if (theme) document.documentElement.dataset.theme = theme;

mountDictionary();

/* ── Вход ──────────────────────────────────────────────────────────────── */

/** Кто вошёл — сразу, не дожидаясь курсов: шапка не должна висеть «Войти». */
let signedIn = null;

async function checkUser() {
  try {
    const response = await fetch("/api/me", { credentials: "same-origin", cache: "no-store" });
    const { user } = await response.json();
    return user ?? null;
  } catch {
    return undefined;
  }
}

function paintUser(user) {
  if (user === undefined) return;
  ui.me.dataset.state = user ? "in" : "out";
  ui.me.textContent = user ? user.name || user.email || "Кабинет" : "Войти";
  ui.me.href = user ? "/#/account" : "/#/login?next=%2Fapp%2F";
}

checkUser().then((user) => {
  signedIn = user ?? null;
  paintUser(user);
});

// Вернулись на вкладку (после входа в другой вкладке или из Telegram) или
// браузер показал страницу из своего снимка — сверяем вход. Поменялся —
// перечитываем страницу: курсы, прогресс и голос зависят от входа.
async function recheckUser() {
  const user = await checkUser();
  if (user === undefined) return;
  if ((user?.id ?? null) !== (signedIn?.id ?? null)) location.reload();
}
document.addEventListener("visibilitychange", () => document.visibilityState === "visible" && recheckUser());
window.addEventListener("pageshow", (event) => event.persisted && recheckUser());

/* ── Мелочи ────────────────────────────────────────────────────────────── */

function el(tag, className = "", text = "") {
  const node = document.createElement(tag);
  if (className) node.className = className;
  if (text) node.textContent = text;
  if (tag === "button") node.type = "button";
  return node;
}

const escapeHtml = (text) => text.replace(/[&<>"']/g, (ch) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[ch]);

/** Ответ модели: абзацы, списки, код, жирный. Всё прочее экранировано. */
function markdown(source) {
  const inline = (text) =>
    escapeHtml(text)
      .replace(/`([^`]+)`/g, "<code>$1</code>")
      .replace(/\*\*([^*]+)\*\*/g, "<strong>$1</strong>");
  const out = [];
  let listItems = null;
  let code = null;
  const flush = () => {
    if (listItems) out.push(`<ul>${listItems.map((item) => `<li>${inline(item)}</li>`).join("")}</ul>`);
    listItems = null;
  };
  for (const line of String(source).replace(/\r/g, "").split("\n")) {
    if (code) {
      if (line.startsWith("```")) {
        out.push(`<pre><code>${escapeHtml(code.join("\n"))}</code></pre>`);
        code = null;
      } else code.push(line);
      continue;
    }
    if (line.startsWith("```")) {
      flush();
      code = [];
      continue;
    }
    const item = /^\s*(?:[-*]|\d+\.)\s+(.*)$/.exec(line);
    if (item) {
      (listItems ??= []).push(item[1]);
      continue;
    }
    flush();
    const heading = /^#{1,4}\s+(.*)$/.exec(line);
    if (heading) out.push(`<h3>${inline(heading[1])}</h3>`);
    else if (line.trim()) out.push(`<p>${inline(line)}</p>`);
  }
  flush();
  if (code) out.push(`<pre><code>${escapeHtml(code.join("\n"))}</code></pre>`);
  return out.join("");
}

function note(node, text, kind = "") {
  node.hidden = !text;
  node.textContent = text;
  node.className = `note${kind ? ` note--${kind}` : ""}`;
}

/* ── Модель ────────────────────────────────────────────────────────────── */

const HINTS = {
  openrouter:
    "Ключ бесплатно: openrouter.ai → Keys. Модели с пометкой «free» ничего не стоят. Ключ хранится только в этом браузере.",
  openai:
    "Любой адрес с /chat/completions: OpenAI (https://api.openai.com/v1), Groq, DeepSeek, Gemini, LM Studio. Ключ хранится только в этом браузере.",
  bridge:
    "Ваши подписки через мост на сервере: claude-code — Claude, codex — ChatGPT, gemini, qwen. Ключ не нужен — пускает вход на сайт. Первый ответ может идти до минуты.",
  ollama:
    "Модель на вашем компьютере, без ключа. Один раз разрешите сайту обращаться к Ollama: в Windows — команда setx OLLAMA_ORIGINS \"https://noahlab.ru\" и перезапуск Ollama.",
};

for (const [kind, provider] of Object.entries(PROVIDERS)) {
  if (provider.ownerOnly) continue;
  const option = el("option", "", provider.title);
  option.value = kind;
  ui.provider.append(option);
}
// Мост — только тем, кого пускает сервер: в списке он первый, раз он есть.
// Гостю мост не положен — и незачем спрашивать сервер.
checkUser()
  .then((user) => (user ? bridgeAvailable() : false))
  .then((ok) => {
  if (!ok) return;
  const option = el("option", "", PROVIDERS.bridge.title);
  option.value = "bridge";
  ui.provider.prepend(option);
  const current = loadModel();
  if (current?.kind === "bridge") {
    ui.provider.value = "bridge";
    paintProvider();
    ui.model.value = current.model;
    refreshModels();
  } else if (!current) {
    // Модели в этом браузере ещё нет, а мост есть — подключаем сразу: у
    // владельца на новом устройстве Ноа должна отвечать без настройки.
    const bridge = { kind: "bridge", base: "", key: "", model: "claude-code-bridge" };
    saveModel(bridge);
    ui.provider.value = "bridge";
    ui.base.value = "";
    ui.model.value = bridge.model;
    paintProvider();
    paintStatus();
    refreshModels();
  }
});

function formModel() {
  return { kind: ui.provider.value, base: ui.base.value.trim(), key: ui.key.value.trim(), model: ui.model.value.trim() };
}

function paintProvider() {
  const kind = ui.provider.value;
  const provider = PROVIDERS[kind];
  ui.baseField.hidden = kind === "openrouter" || kind === "bridge";
  ui.keyField.hidden = kind === "ollama" || kind === "bridge";
  if (kind === "ollama" && !ui.base.value) ui.base.value = provider.base;
  ui.key.placeholder = provider.keyHint;
  ui.providerHint.textContent = HINTS[kind];
}

function paintStatus() {
  const model = loadModel();
  ui.modelStatus.textContent = model ? `${PROVIDERS[model.kind].title.split(" (")[0]} · ${model.model}` : "не подключена";
  ui.modelStatus.classList.toggle("status--on", Boolean(model));
}

let listTimer = 0;
/** Список моделей провайдера — подсказкой в поле. */
function refreshModels() {
  clearTimeout(listTimer);
  listTimer = setTimeout(async () => {
    const draft = formModel();
    if (PROVIDERS[draft.kind].needsKey && !draft.key) return;
    try {
      const models = await listModels(draft);
      ui.models.replaceChildren(
        ...models.slice(0, 400).map((m) => {
          const option = el("option");
          option.value = m.id;
          if (m.free && draft.kind !== "ollama") option.label = `${m.id} — бесплатно`;
          return option;
        }),
      );
      if (!ui.model.value && models[0]) ui.model.value = models[0].id;
    } catch {
      ui.models.replaceChildren();
    }
  }, 400);
}

const saved = loadModel();
// Моста в списке ещё нет — он появится, когда сервер скажет, что пускает.
ui.provider.value = saved?.kind === "bridge" ? "openrouter" : (saved?.kind ?? "openrouter");
ui.base.value = saved?.base ?? "";
ui.key.value = saved?.key ?? "";
ui.model.value = saved?.model ?? "";
paintProvider();
paintStatus();
if (saved) refreshModels();

ui.provider.addEventListener("change", () => {
  ui.base.value = "";
  ui.model.value = "";
  paintProvider();
  refreshModels();
});
ui.key.addEventListener("change", refreshModels);
ui.base.addEventListener("change", refreshModels);

ui.modelForm.addEventListener("submit", async (event) => {
  event.preventDefault();
  const draft = formModel();
  if (PROVIDERS[draft.kind].needsKey && !draft.key) return note(ui.modelNote, "Впишите ключ.", "error");
  if (draft.kind !== "openrouter" && draft.kind !== "bridge" && !draft.base) return note(ui.modelNote, "Впишите адрес.", "error");
  if (!draft.model) return note(ui.modelNote, "Выберите модель.", "error");
  note(ui.modelNote, "Проверяю…");
  try {
    const reply = await chat(draft, [{ role: "user", content: "Ответь одним словом: готово" }], { maxTokens: 20, timeoutMs: 45_000 });
    saveModel(draft);
    paintStatus();
    note(ui.modelNote, `Работает. Модель ответила: «${reply.slice(0, 60)}»`, "ok");
  } catch (err) {
    note(ui.modelNote, err.message, "error");
  }
});

ui.forget.addEventListener("click", () => {
  saveModel(null);
  ui.key.value = "";
  ui.model.value = "";
  paintStatus();
  note(ui.modelNote, "Модель забыта в этом браузере.");
});

/* ── Разговор ──────────────────────────────────────────────────────────── */

const CHAT = "noa.chat";
/** Сколько обменов помнить: дальше модель пересказывает прежнее. */
const DEPTH = 10;

let thread = [];
try {
  thread = JSON.parse(sessionStorage.getItem(CHAT) ?? "[]");
} catch {
  thread = [];
}

const PERSONA =
  "Тебя зовут Ноа, ты помощник NOAH. Сейчас человек говорит с тобой на сайте, в браузере: действий на его " +
  "компьютере здесь нет — открыть программу, найти файл или напомнить умеет программа Ноа на компьютере, её можно " +
  "скачать на noahlab.ru. Отвечай по делу, живым языком, по-русски. Коротко, если вопрос простой; подробно и по " +
  "шагам, если человек разбирается в теме или просит объяснить. Можно списки и `код`. Без вступлений и без " +
  "предложений помочь ещё.";

function line(who, text) {
  const node = el("div", `msg msg--${who}`);
  if (who === "noa") node.innerHTML = markdown(text);
  else node.textContent = text;
  ui.log.append(node);
  ui.log.scrollTop = ui.log.scrollHeight;
  return node;
}

function paintThread() {
  ui.log.replaceChildren();
  if (!thread.length) {
    line("hello", "Привет! Я Ноа. Нажмите «Поговорить голосом» или напишите вопрос. Курсы — в разделе «Обучение».");
  }
  for (const item of thread) {
    line("me", item.q);
    line("noa", item.a);
  }
}
paintThread();

function speak(text) {
  if (ui.speakAnswers.checked) sayAloud(text);
}

try {
  ui.speakAnswers.checked = localStorage.getItem("noa.speak") === "1";
} catch {
  /* по умолчанию молчим */
}
ui.speakAnswers.addEventListener("change", () => {
  try {
    localStorage.setItem("noa.speak", ui.speakAnswers.checked ? "1" : "0");
  } catch {
    /* не запомнится — не беда */
  }
  if (!ui.speakAnswers.checked) stopSpeaking();
});

/**
 * Вопрос модели от имени разговора: реплики — в ленту, ответ — в историю.
 * Общий для текста и голоса: сказанное голосом видно в той же ленте.
 */
async function askModel(said) {
  const model = loadModel();
  if (!model) throw new Error("Сначала подключите модель — в разделе «Модель».");
  if (!thread.length) ui.log.replaceChildren();
  line("me", said);
  const wait = line("wait", "Ноа думает…");
  const messages = [{ role: "system", content: PERSONA }];
  for (const item of thread) messages.push({ role: "user", content: item.q }, { role: "assistant", content: item.a });
  messages.push({ role: "user", content: said });
  try {
    const reply = await chat(model, messages, { maxTokens: 1500 });
    line("noa", reply);
    thread = [...thread, { q: said, a: reply }].slice(-DEPTH);
    sessionStorage.setItem(CHAT, JSON.stringify(thread));
    return reply;
  } finally {
    wait.remove();
  }
}

async function send(text) {
  const said = text.trim();
  if (!said || ui.send.disabled) return;
  if (!loadModel()) {
    line("error", "Сначала подключите модель — в разделе «Модель».");
    ui.provider.focus();
    return;
  }
  ui.input.value = "";
  ui.send.disabled = true;
  try {
    speak(await askModel(said));
  } catch (err) {
    line("error", err.message);
  } finally {
    ui.send.disabled = false;
    ui.input.focus();
  }
}

ui.form.addEventListener("submit", (event) => {
  event.preventDefault();
  send(ui.input.value);
});
ui.input.addEventListener("keydown", (event) => {
  if (event.key === "Enter" && !event.shiftKey) {
    event.preventDefault();
    send(ui.input.value);
  }
});
ui.clear.addEventListener("click", () => {
  thread = [];
  sessionStorage.removeItem(CHAT);
  paintThread();
});

// «Поговорить» — разговор голосом без рук, а не диктовка одной фразы:
// диктовка и так есть на клавиатуре телефона.
if (!canListen) ui.mic.hidden = true;
ui.mic.prepend(orbIcon());
ui.mic.classList.toggle("is-off", !canListen);
ui.mic.addEventListener("click", () => {
  if (!loadModel()) {
    line("error", "Сначала подключите модель — в разделе «Модель».");
    ui.provider.focus();
    return;
  }
  stopSpeaking();
  try {
    startTalk({ title: "Разговор с Ноа", greeting: "Говорите — я слушаю.", reply: askModel });
  } catch (err) {
    line("error", err.message);
  }
});

/* ── Обучение ──────────────────────────────────────────────────────────── */

async function paintCourses() {
  let noa;
  try {
    noa = await openNoa();
  } catch (err) {
    note(ui.courseNote, `Курсы не загрузились: ${err.message}`, "error");
    return;
  }
  paintUser(noa.user);
  ui.loginHint.hidden = Boolean(noa.user);
  const cards = noa.learning.overview();
  if (!cards.length) {
    ui.courses.replaceChildren(el("p", "empty", "Курсов пока нет. Нажмите «Собрать курс» — напишите, о чём он, и Ноа соберёт его сама."));
    return;
  }
  ui.courses.replaceChildren(
    ...cards.map((course) => {
      const item = el("article", "course");
      const head = el("div", "course__head");
      const title = el("a", "course__title", course.title);
      title.href = `./learning.html?course=${encodeURIComponent(course.id)}`;
      head.append(title, el("span", "course__percent", `${course.percent}%`));
      const bar = el("div", "bar-line");
      const fill = el("span");
      fill.style.width = `${course.percent}%`;
      bar.append(fill);
      const due = course.mastery.due ? ` · повторить: ${course.mastery.due}` : "";
      const growing = course.building ? ` · собирается: готово ${course.building.done} из ${course.building.total}` : "";
      const meta = el("p", "course__meta", `Тем: ${course.topics.length}${due}${growing}`);
      if (course.building) item.classList.add("course--building");
      const actions = el("div", "row");
      const open = el("a", "btn btn--primary", course.percent ? "Продолжить" : "Начать");
      open.href = title.href;
      const remove = el("button", "btn btn--quiet", "Убрать");
      remove.addEventListener("click", async () => {
        if (!confirm(`Убрать курс «${course.title}»? Прогресс сохранится — если курс вернётся, он продолжится с того же места.`)) return;
        await noa.removeCourse(course.id).catch((err) => note(ui.courseNote, err.message, "error"));
        paintCourses();
      });
      actions.append(open, remove);
      item.append(head, bar, meta, actions);
      return item;
    }),
  );
}

ui.addCourse.addEventListener("click", () => ui.courseFile.click());
ui.courseFile.addEventListener("change", async () => {
  const file = ui.courseFile.files?.[0];
  ui.courseFile.value = "";
  if (!file) return;
  if (file.size > 3_000_000) return note(ui.courseNote, "Файл больше 3 МБ — это не похоже на курс.", "error");
  try {
    const course = JSON.parse(await file.text());
    const noa = await openNoa();
    note(ui.courseNote, await noa.addCourse(course), "ok");
    paintCourses();
  } catch (err) {
    note(ui.courseNote, err instanceof SyntaxError ? "Это не JSON-файл курса." : err.message, "error");
  }
});

/* ── Сборка курса ─────────────────────────────────────────────────────────
   С мостом курс собирается на сервере: можно закрыть страницу, вернуться —
   сборка идёт. Без моста — моделью человека прямо в этой вкладке, и её
   нельзя закрывать. В обоих случаях курс появляется после первой темы, а
   остальные дорастают на глазах: список сверяется с аккаунтом сам. */

/** Сборки на сервере (последние) и сборка в этой вкладке. */
let serverBuilds = [];
let localBuild = null;
let bridgeOk = null;

async function canBuildOnServer() {
  const noa = await openNoa();
  if (!noa.user) return false;
  bridgeOk ??= await bridgeAvailable();
  return bridgeOk;
}

/** Движок моста для сборки: выбранный, если выбран мост, иначе Claude. */
function bridgeModel() {
  const model = loadModel();
  return model?.kind === "bridge" && model.model ? model.model.replace(/:free$/, "") : "claude-code-bridge";
}

function buildCard({ title, message, status, done, total, courseId, onStop }) {
  const card = el("article", `build build--${status}`);
  const head = el("div", "build__head");
  head.append(el("strong", "build__title", title || "Новый курс"), el("span", "build__status", STATUS[status] ?? status));
  card.append(head);
  if (total) {
    const bar = el("div", "bar-line");
    const fill = el("span");
    fill.style.width = `${Math.round((done / total) * 100)}%`;
    bar.append(fill);
    card.append(bar);
  }
  card.append(el("p", "build__message", message || ""));
  const actions = el("div", "row");
  if (status === "running" && onStop) {
    const stop = el("button", "btn btn--quiet", "Остановить");
    stop.addEventListener("click", onStop);
    actions.append(stop);
  }
  if (courseId && done > 0) {
    const open = el("a", "btn", status === "running" ? "Учиться по готовым темам" : "Открыть курс");
    open.href = `./learning.html?course=${encodeURIComponent(courseId)}`;
    actions.append(open);
  }
  if (actions.childNodes.length) card.append(actions);
  return card;
}

const STATUS = { running: "собирается", done: "готово", failed: "не вышло", stopped: "остановлена" };

function paintBuilds() {
  const fresh = Date.now() - 60 * 60 * 1000;
  const shown = serverBuilds.filter((b) => b.status === "running" || Date.parse(`${String(b.updated).replace(" ", "T")}Z`) > fresh).slice(0, 2);
  const cards = shown.map((b) =>
    buildCard({
      ...b,
      onStop: async () => {
        const noa = await openNoa();
        await noa.stopBuild(b.id).catch((err) => note(ui.courseNote, err.message, "error"));
        tick();
      },
    }),
  );
  if (localBuild) {
    cards.unshift(
      buildCard({
        ...localBuild.state,
        onStop: () => {
          localBuild.stop = true;
        },
      }),
    );
  }
  ui.builds.replaceChildren(...cards);
}

async function runLocalBuild(goal) {
  const model = loadModel();
  if (!model) throw new Error("Сначала подключите модель — в разделе «Модель».");
  const noa = await openNoa();
  const format = await noa.courseFormat();
  const control = { stop: false, state: { status: "running", message: "Составляю план курса", done: 0, total: 0, title: "", courseId: "" } };
  localBuild = control;
  paintBuilds();
  buildCourse({
    goal,
    format,
    chat: (messages, opts) => chat(model, messages, { maxTokens: opts?.maxTokens ?? 4000, json: opts?.json, timeoutMs: opts?.long ? 600_000 : 120_000 }),
    save: async (course) => {
      await noa.saveBuilding(course);
      paintCourses();
    },
    progress: (p) => {
      Object.assign(control.state, { message: p.message, done: p.done, total: p.total, title: p.title ?? control.state.title, courseId: p.courseId ?? control.state.courseId });
      paintBuilds();
    },
    stopped: () => control.stop,
    taken: new Set(noa.courses().map((c) => c.id)),
  })
    .then(() => (control.state.status = "done"))
    .catch((err) => {
      control.state.status = err instanceof Stopped ? "stopped" : "failed";
      control.state.message = err instanceof Stopped ? "Сборка остановлена — готовые темы остались." : err.message;
    })
    .finally(() => {
      paintBuilds();
      paintCourses();
    });
}

// Сборка в этой вкладке — закрыть её значит прервать сборку.
window.addEventListener("beforeunload", (event) => {
  if (localBuild?.state.status === "running") {
    event.preventDefault();
    event.returnValue = "";
  }
});

ui.buildOpen.addEventListener("click", async () => {
  ui.buildForm.hidden = false;
  ui.courseActions.hidden = true;
  const server = await canBuildOnServer();
  // Выбор модели — только у Claude через мост: у своей модели он один.
  ui.qualityField.hidden = !(server && bridgeModel().startsWith("claude"));
  ui.buildHint.textContent = server
    ? "Соберу через мост на сервере: около минуты на тему с Sonnet. Страницу можно закрыть — курс появится после первой темы, остальные дорастут сами."
    : loadModel()
      ? "Соберу моделью, подключённой в этом браузере: не закрывайте вкладку, пока идёт сборка. Курс появится после первой темы."
      : "Сначала подключите модель — в разделе «Модель».";
  ui.buildGoal.focus();
});

ui.buildCancel.addEventListener("click", () => {
  ui.buildForm.hidden = true;
  ui.courseActions.hidden = false;
});

ui.buildForm.addEventListener("submit", async (event) => {
  event.preventDefault();
  const goal = ui.buildGoal.value.trim();
  if (goal.length < 10) return note(ui.courseNote, "Опишите курс подробнее: о чём он и для чего — хотя бы одним предложением.", "error");
  note(ui.courseNote, "");
  try {
    if (await canBuildOnServer()) {
      const noa = await openNoa();
      await noa.startBuild(goal, bridgeModel(), ui.buildQuality.value);
    } else {
      if (localBuild?.state.status === "running") throw new Error("Уже собирается курс — дождитесь его или остановите.");
      await runLocalBuild(goal);
    }
    ui.buildGoal.value = "";
    ui.buildForm.hidden = true;
    ui.courseActions.hidden = false;
    tick();
  } catch (err) {
    note(ui.courseNote, err.message, "error");
  }
});

/* ── Живой список курсов ─────────────────────────────────────────────────
   Курс дописывает сборка или нейросеть по MCP — страница сверяется с
   аккаунтом сама: часто, пока идёт сборка, и раз в полминуты в остальное
   время. Сверка — по версии курсов, весь список качается, только когда
   она сменилась. */

let tickTimer = 0;

async function tick() {
  clearTimeout(tickTimer);
  const noa = await openNoa().catch(() => null);
  if (!noa?.user) return;
  const [builds, changed] = await Promise.all([noa.builds(), noa.refresh()]);
  serverBuilds = builds;
  paintBuilds();
  if (changed) paintCourses();
  const active = builds.some((b) => b.status === "running");
  if (document.visibilityState === "visible") tickTimer = setTimeout(tick, active ? 4000 : 30_000);
}

document.addEventListener("visibilitychange", () => {
  if (document.visibilityState === "visible") tick();
});

paintCourses();
tick();
