// Главная Ноа в браузере: разговор, своя модель и курсы обучения.
//
// Всё, что в программе делает Rust, здесь делает браузер: модель зовётся
// прямо со страницы (ai-web.js), курсы и прогресс — noa-store.js. Сайт
// видит только курсы и прогресс вошедшего человека, но не его ключ.

import { PROVIDERS, loadModel, saveModel, listModels, chat, bridgeAvailable } from "./ai-web.js";
import { openNoa } from "./noa-store.js";
import { mountDictionary } from "./web-api.js";
import { speak as sayAloud, stopSpeaking, listen, canListen } from "./voice.js";

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
bridgeAvailable().then((ok) => {
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
    ui.provider.value = "bridge";
    ui.base.value = "";
    paintProvider();
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
    line("hello", "Привет! Я Ноа. Спросите что угодно или откройте курс справа — разберём урок вместе.");
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

async function send(text) {
  const said = text.trim();
  if (!said || ui.send.disabled) return;
  const model = loadModel();
  if (!model) {
    line("error", "Сначала подключите модель — справа, в разделе «Модель».");
    ui.provider.focus();
    return;
  }
  ui.input.value = "";
  if (!thread.length) ui.log.replaceChildren();
  line("me", said);
  const wait = line("wait", "Ноа думает…");
  ui.send.disabled = true;
  const messages = [{ role: "system", content: PERSONA }];
  for (const item of thread) messages.push({ role: "user", content: item.q }, { role: "assistant", content: item.a });
  messages.push({ role: "user", content: said });
  try {
    const reply = await chat(model, messages, { maxTokens: 1500 });
    wait.remove();
    line("noa", reply);
    thread = [...thread, { q: said, a: reply }].slice(-DEPTH);
    sessionStorage.setItem(CHAT, JSON.stringify(thread));
    speak(reply);
  } catch (err) {
    wait.remove();
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

if (!canListen) ui.mic.hidden = true;
ui.mic.addEventListener("click", async () => {
  if (ui.mic.disabled) return;
  stopSpeaking();
  ui.mic.disabled = true;
  ui.mic.textContent = "Слушаю…";
  try {
    const said = await listen({ onHeard: (text) => (ui.input.value = text) });
    if (said) {
      // Спросили голосом — отвечаем голосом.
      ui.speakAnswers.checked = true;
      await send(said);
    }
  } catch (err) {
    line("error", err.message);
  } finally {
    ui.mic.disabled = false;
    ui.mic.textContent = "🎙 Голосом";
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
  if (noa.user) {
    ui.me.textContent = noa.user.name || noa.user.email || "Кабинет";
    ui.me.href = "/#/account";
  }
  ui.loginHint.hidden = Boolean(noa.user);
  const cards = noa.learning.overview();
  if (!cards.length) {
    ui.courses.replaceChildren(el("p", "empty", "Курсов пока нет. Добавьте файл курса или попросите свою нейросеть собрать курс — как, написано ниже."));
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
      const meta = el("p", "course__meta", `Тем: ${course.topics.length}${due}`);
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

paintCourses();
