// Команды окон Ноа в браузере — вместо Rust.
//
// Окно обучения зовёт `api.invoke("learn_…")`, как в программе, и не знает,
// где оно открыто. В программе ответ даёт Rust, здесь — learn-core.js через
// этот модуль. Отказ приходит строкой, как из Tauri: окно показывает его
// `String(err)`.
//
// Сюда же — словарь: выделение с Ctrl (на телефоне — меню у выделения)
// открывает то же окно объяснения, что и в программе.

import { openNoa } from "./noa-store.js";
import { WebHost } from "../web-host.js";
import { speak, stopSpeaking, speaking, canListen, dictate, listen } from "./voice.js";
import { startTalk } from "./talk.js";
import { splitScheme } from "../scheme.js";
import { dictionaryClient, loadModel, saveModel, bridgeAvailable, chat } from "./ai-web.js";
import { request } from "./net.js";
import { sections as lessonSections } from "./learn-core.js";

const listeners = new Map();

function emit(event, payload) {
  for (const handler of listeners.get(event) ?? []) handler({ event, payload });
}

/* ── Голос ──────────────────────────────────────────────────────────────── */


/** Диктовка ответа: запись идёт, пока не нажали «Готово». */
let dictation = null;

async function dictateStart() {
  if (dictation) return;
  dictation = await dictate();
}

async function dictateStop() {
  if (!dictation) throw new Error("Запись не шла.");
  const current = dictation;
  dictation = null;
  const text = await current.stop();
  if (!text) throw new Error("Ничего не расслышал — попробуйте ещё раз.");
  return text;
}

/** Обсуждение урока голосом: тот же экран разговора, отвечает репетитор. */
/** С чего Ноа начинает разбор — она ведёт, а не ждёт вопроса. */
const OPENING =
  "Начни живой разбор: в двух-трёх фразах — о чём это и где пригодится на практике и на собеседовании, " +
  "и задай мне первый вопрос, чтобы понять, что я уже знаю.";

async function discussByVoice(learning, target) {
  const greeting = await learning.ask(target, OPENING, { voice: true }).catch(() => "Слушаю. Что разобрать?");
  startTalk({
    title: "Разбор с Ноа",
    greeting,
    reply: (said) => learning.ask(target, said, { voice: true }),
    // Сказанное голосом — в ту же ленту обсуждения, что и напечатанное.
    onExchange: (said, answer) => emit("learn:talk", { q: said, a: answer }),
  });
}

/* ── Урок с Ноа голосом ─────────────────────────────────────────────────── */

/** Идущий урок голосом — чтобы «Закончить» его остановил. */
let walking = null;

/**
 * Урок с Ноа без рук: Ноа рассказывает раздел, слушает пересказ, разбирает и
 * ведёт дальше. Сказанное приходит в окно событием learn:talk. Две паузы
 * подряд или «стоп», «хватит», «спасибо» — конец; продолжить можно текстом.
 */
async function walkByVoice(learning, target, listenToo) {
  // Схему раздела рисует окно, вслух она не читается.
  const opening = splitScheme(await learning.walkStart(target));
  const first = opening.text;
  emit("learn:talk", { q: "", a: first, section: learning.walkSection(), scheme: opening.scheme });
  if (!listenToo || !canListen) return first;
  const session = {};
  walking = session;
  (async () => {
    let text = first;
    let quiet = 0;
    while (walking === session) {
      await speak(text);
      if (walking !== session) break;
      emit("learn:listening", true);
      // Пересказ — с паузами на подумать: конец фразы — после 2,6 с тишины.
      const said = String((await listen({ pause: 2600 }).catch(() => "")) ?? "").trim();
      emit("learn:listening", false);
      if (walking !== session) break;
      if (!said) {
        if (++quiet >= 2) break;
        text = "Я здесь. Перескажите, как поняли, или спросите.";
        continue;
      }
      quiet = 0;
      if (/^(стоп|хватит|спасибо|пока|закончим)/i.test(said)) break;
      try {
        const reply = splitScheme(await learning.ask({ ...target, section: learning.walkSection() ?? target.section }, said, { voice: true }));
        text = reply.text;
        emit("learn:talk", { q: said, a: text, section: learning.walkSection(), scheme: reply.scheme });
      } catch (err) {
        text = `Не получилось ответить: ${err.message ?? err}`;
      }
    }
    if (walking === session) walking = null;
    emit("learn:listening", false);
  })();
  return first;
}

/* ── Устный зачёт ───────────────────────────────────────────────────────── */

/**
 * Устный зачёт: Ноа читает вопрос, слушает ответ, проверяет и говорит итог.
 * «Хватит» разбирает сам зачёт — он подводит итог, — поэтому общие слова
 * прощания здесь выключены.
 */
function oralExam(learning, courseId, topicId) {
  const intro = learning.oralStart(courseId, topicId);
  if (!learning.oralActive()) throw new Error(intro);
  const session = startTalk({
    title: "Устный зачёт",
    intro,
    bye: false,
    reply: (said) => learning.oralAnswer(said),
    onQuiet: () => learning.oralStop() ?? "Закончили.",
  });
  session.done.finally(() => {
    if (learning.oralActive()) learning.oralStop();
    emit("learn:changed", {});
  });
}

/* ── Словарь ────────────────────────────────────────────────────────────── */

function addStyle(href) {
  if (document.querySelector(`link[href="${href}"]`)) return;
  const link = document.createElement("link");
  link.rel = "stylesheet";
  link.href = href;
  document.head.append(link);
}

/** Абзац вокруг выделения — чтобы слово объяснялось в своём смысле. */
function selectionContext() {
  const node = getSelection()?.anchorNode;
  const block = (node?.nodeType === 1 ? node : node?.parentElement)?.closest("p, li, td, pre, h1, h2, h3, blockquote, div");
  return (block?.textContent ?? "").replace(/\s+/g, " ").trim().slice(0, 800);
}

let dictionary = null;

/** Включает объяснение выделенного на этой странице. */
export function mountDictionary() {
  if (dictionary) return dictionary;
  const base = new URL("../../styles/", import.meta.url);
  addStyle(new URL("popup.css", base).href);
  addStyle(new URL("menu.css", base).href);
  // Выделили — рядом «Объяснить»: и на телефоне, и на компьютере, без Ctrl.
  dictionary = new WebHost({
    client: dictionaryClient(),
    requireLeftCtrl: true,
    forceTouchMenu: true,
    onRead: (text) => speak(text),
    onStopRead: () => stopSpeaking(),
  }).mount();
  // Всё, что умеет окно объяснения в программе, — кнопками вместо клавиш:
  // уточнить словами или голосом (зажать микрофон), прочитать вслух.
  const view = dictionary.view;
  view.dialogue = true;
  view.onSpeak = (text) => {
    view.speaking = true;
    speak(text).finally(() => (view.speaking = false));
  };
  view.onStopSpeaking = () => {
    stopSpeaking();
    view.speaking = false;
  };
  view.onMic = async (down) => {
    if (down) {
      view.listening = true;
      await dictateStart().catch(() => (view.listening = false));
      return;
    }
    view.listening = false;
    const text = await dictateStop().catch(() => "");
    if (text) view.askByVoice(text);
  };
  view.onAnswer = (answer) => view.onSpeak(answer);
  view.onOpenSettings = () => (location.href = "/app/#settings");
  const showAt = dictionary.showAt.bind(dictionary);
  dictionary.showAt = (anchor, term, context = "") => showAt(anchor, term, context || selectionContext());
  return dictionary;
}

/* ── Команды ────────────────────────────────────────────────────────────── */

/* ── Модель ─────────────────────────────────────────────────────────────── */

let modelChecked = false;

/**
 * Модели в этом браузере нет — подключаем мост, если сервер к нему пускает,
 * иначе показываем плашку со ссылкой. Без модели не работают проверка
 * ответов, обсуждение и разбор, и молча падать на каждом вопросе хуже.
 */
async function ensureModel(user) {
  if (modelChecked || loadModel()) return;
  modelChecked = true;
  if (user && (await bridgeAvailable())) {
    saveModel({ kind: "bridge", base: "", key: "", model: "claude-code-bridge" });
    return;
  }
  const bar = document.createElement("div");
  bar.className = "model-missing";
  const link = document.createElement("a");
  link.href = "./#settings";
  link.textContent = "Подключить модель";
  bar.append("Модель не подключена — проверка ответов, обсуждение и разбор не заработают. ", link);
  document.body.prepend(bar);
}

/* ── Курс дорастает ─────────────────────────────────────────────────────
   Курс дописывает сборка или нейросеть по MCP — окно сверяется с аккаунтом
   само и говорит окну обучения обновиться: чаще, пока курс собирается. */

let pollTimer = 0;

function schedulePoll(noa, building) {
  clearTimeout(pollTimer);
  if (!noa.user) return;
  pollTimer = setTimeout(async () => {
    if (document.visibilityState === "visible" && (await noa.refresh())) emit("learn:changed", {});
    else schedulePoll(noa, building);
  }, building ? 8000 : 30_000);
}

// Без входа курсы лежат в браузере: сборка в другой вкладке дописала тему —
// браузер скажет об этом событием storage.
addEventListener("storage", async (event) => {
  if (event.key !== "noa.courses") return;
  const noa = await openNoa();
  if (await noa.refresh()) emit("learn:changed", {});
});

/* ── Приложение «NOAH Учёба» в браузере (learn.html) ──────────────────────
   Те же экраны, что на iPhone; команды телефона — здесь, браузерными
   средствами: вход — на сайте, сборки курсов — сервер, ассистент — модель
   Ноа онлайн, микрофон — распознавание браузера. */

/** JSON с сайта под входом этого браузера. */
async function site(path, body) {
  const response = await request(path, {
    method: body ? "POST" : "GET",
    headers: body ? { "Content-Type": "application/json" } : {},
    body: body ? JSON.stringify(body) : undefined,
    credentials: "same-origin",
    timeout: 20_000,
    retries: body ? 0 : 1,
  });
  const data = await response.json().catch(() => ({}));
  if (!response.ok) throw new Error(data.error ?? `Ошибка ${response.status}`);
  return data;
}

/* ── Путеводители — в аккаунте, собирает сервер ───────────────────────────
   Пока какой-то пишется, вкладка сверяется раз в несколько секунд и
   говорит окну о новых разделах — как программа событием guides:changed. */

let guideMarks = "";
let guideTimer = 0;
const guideMark = (card) => `${card.id}:${card.status}:${card.written}:${card.sections};`;

function noteGuides(cards) {
  for (const card of cards) if (!guideMarks.includes(guideMark(card))) emit("guides:changed", card.id);
  guideMarks = cards.map(guideMark).join("");
  if (cards.some((card) => card.status === "building")) watchGuides();
}

function watchGuides() {
  clearTimeout(guideTimer);
  guideTimer = setTimeout(async () => {
    try {
      noteGuides((await site(`/api/noa/guides?_=${Date.now()}`)).guides ?? []);
    } catch {
      watchGuides();
    }
  }, 4000);
}

async function guides(noa, cmd, args) {
  const id = encodeURIComponent(String(args.id ?? ""));
  switch (cmd) {
    case "guides_list": {
      if (!noa.user) return [];
      const cards = (await site(`/api/noa/guides?_=${Date.now()}`)).guides ?? [];
      guideMarks = cards.map(guideMark).join("");
      if (cards.some((card) => card.status === "building")) watchGuides();
      return cards;
    }
    case "guide_get": {
      const { guide } = await site(`/api/noa/guides/${id}?_=${Date.now()}`);
      if (guide.status === "building") watchGuides();
      return guide;
    }
    case "guide_build": {
      if (!noa.user) throw new Error("Путеводители собираются на сервере — войдите на сайт.");
      const made = await site("/api/noa/guides", { goal: args.goal });
      emit("guides:changed", made.id);
      watchGuides();
      return made.id;
    }
    case "guide_resume":
      await site(`/api/noa/guides/${id}/resume`, { wish: args.wish ?? "" });
      emit("guides:changed", args.id);
      watchGuides();
      return null;
    case "guide_stop":
      await site("/api/noa/guides/stop", {});
      return null;
    case "guide_delete":
      await site(`/api/noa/guides/${id}/remove`, {});
      emit("guides:changed", args.id);
      return null;
    case "guide_picture":
      // Браузер берёт картинку по адресу сам, со своим кэшем.
      return `/api/noa/guides/${id}/pictures/${encodeURIComponent(String(args.file ?? ""))}`;
    case "guide_source":
      if (/^https:\/\/[\w.-]+\.wikipedia\.org\//.test(String(args.url))) open(args.url, "_blank", "noopener");
      return null;
    case "guide_wanted":
      return null;
    default:
      return undefined;
  }
}

/** Разговор с ассистентом — последние обмены, чтобы «а это?» было понятно. */
const assistantThread = [];

const ASSISTANT =
  "Тебя зовут Ноа. Ты — помощник человека в учёбе: курсы, уроки, практика, подготовка к собеседованию. " +
  "Отвечай по-русски, коротко и разговорно, одной-тремя фразами, без разметки и списков — ответ читают вслух. " +
  "Не предлагай помощь в конце и не представляйся.";

async function assistantAnswer(text) {
  const model = loadModel();
  if (!model) throw new Error("Модель не подключена — войдите на сайт или подключите её в Ноа онлайн.");
  const messages = [{ role: "system", content: ASSISTANT }];
  for (const item of assistantThread.slice(-6)) messages.push({ role: "user", content: item.q }, { role: "assistant", content: item.a });
  messages.push({ role: "user", content: text });
  const answer = String(await chat(model, messages, { maxTokens: 400 })).trim();
  assistantThread.push({ q: text, a: answer });
  return answer;
}

/** Слушание микрофона: отменить — «Закрыть» или новая фраза. */
let hearing = null;

/** Тихий сигнал — конец отрезка фокуса. */
function chime() {
  try {
    const ctx = new AudioContext();
    [0, 0.28].forEach((delay, at) => {
      const osc = ctx.createOscillator();
      const gain = ctx.createGain();
      osc.frequency.value = at ? 880 : 660;
      gain.gain.setValueAtTime(0.0001, ctx.currentTime + delay);
      gain.gain.exponentialRampToValueAtTime(0.18, ctx.currentTime + delay + 0.02);
      gain.gain.exponentialRampToValueAtTime(0.0001, ctx.currentTime + delay + 0.9);
      osc.connect(gain).connect(ctx.destination);
      osc.start(ctx.currentTime + delay);
      osc.stop(ctx.currentTime + delay + 1);
    });
    setTimeout(() => ctx.close(), 1600);
  } catch {
    /* без звука */
  }
}

async function learnApp(noa, cmd, args) {
  switch (cmd) {
    case "account_status":
      return Boolean(noa.user);
    case "account_name":
      return noa.user?.name ?? "";
    case "account_login":
      // Вход — на сайте (Telegram или Google), потом обратно сюда.
      location.href = `/#/login?next=${encodeURIComponent(location.pathname + location.search)}`;
      return new Promise(() => {});
    case "app_version":
      return "веб";
    // Какая модель отвечает в браузере: мост (вошли владельцем) или своя.
    case "web_model": {
      const model = loadModel();
      return model ? { kind: model.kind, model: model.model ?? "" } : null;
    }
    case "learn_builds":
      return noa.user ? ((await site("/api/noa/builds")).builds ?? []) : [];
    case "learn_build":
      if (!noa.user) throw new Error("Сборка курса — после входа на сайт.");
      return site("/api/noa/builds", { goal: args.goal, quality: args.quality ?? "sonnet" });
    case "learn_build_stop":
      return site(`/api/noa/builds/${encodeURIComponent(args.id)}/stop`, {});
    case "learn_commands":
      if (!noa.user) throw new Error("Команды урока собирает сервер — войдите на сайт.");
      return (await site("/api/noa/commands", { course: args.course, topic: args.topic })).commands;
    case "learn_build_resume":
      return (await site(`/api/noa/builds/${encodeURIComponent(args.id)}/resume`, {})).build;
    case "learn_build_remove":
      return site(`/api/noa/builds/${encodeURIComponent(args.id)}/remove`, {});
    case "ai_explain":
      return dictionaryClient().explain(args.term, args.context);
    case "ai_ask":
      return dictionaryClient().ask(args.term, args.context, args.thread ?? [], args.question);
    case "phone_ask":
      return assistantAnswer(String(args.text ?? ""));
    // Потока в браузере нет: ответ целиком, вслух его читает окно.
    case "phone_talk":
      return { text: await assistantAnswer(String(args.text ?? "")), spoken: false };
    // Тренировки «Наизусть» и «Собеседование»: модель по указаниям экрана.
    case "learn_coach": {
      const model = loadModel();
      if (!model) throw new Error("Модель не подключена — войдите на сайт или подключите её в Ноа онлайн.");
      const messages = [{ role: "system", content: String(args.rules ?? "") }];
      for (const item of (args.thread ?? []).slice(-10)) messages.push({ role: "user", content: item.q }, { role: "assistant", content: item.a });
      messages.push({ role: "user", content: String(args.said ?? "") });
      return String(await chat(model, messages, { maxTokens: args.long ? 2500 : 900, timeoutMs: 180_000 }));
    }
    case "plugin:sufler|listen": {
      hearing?.abort();
      hearing = new AbortController();
      const text = await listen({ signal: hearing.signal, ...(args.pause ? { pause: args.pause * 1000 } : {}) });
      return { text: String(text ?? "") };
    }
    case "plugin:sufler|cancelListening":
    case "plugin:sufler|stopListening":
      hearing?.abort();
      return null;
    case "plugin:sufler|chime":
      chime();
      return null;
    // Практика — в программе на компьютере и в приложении на iPhone: в
    // браузере своего терминала нет.
    case "practice_progress":
      return [];
    // Удалить курс: из аккаунта и браузера (убранный можно вернуть в Ноа онлайн).
    // «Потянуть вниз» на экране курсов: свежая версия курсов аккаунта.
    case "learn_sync":
      return noa.refresh();
    case "learn_course_remove":
      await noa.removeCourse(String(args.course));
      emit("learn:changed", {});
      return null;
    default:
      return undefined;
  }
}

/** Курс, открытый по ссылке `?course=`, — первым: окно берёт первый. */
const wanted = new URLSearchParams(location.search).get("course");

async function run(cmd, args = {}) {
  const noa = await openNoa();
  ensureModel(noa.user);
  const l = noa.learning;
  const own = await learnApp(noa, cmd, args);
  if (own !== undefined) return own;
  const guide = await guides(noa, cmd, args);
  if (guide !== undefined) return guide;
  switch (cmd) {
    case "runtime_config":
      return { theme: localStorage.getItem("noa.theme") || "noah", language: "ru" };
    case "learn_overview": {
      await noa.refresh();
      const all = l.overview();
      schedulePoll(noa, all.some((c) => c.building));
      const at = all.findIndex((c) => c.id === wanted);
      if (at > 0) all.unshift(...all.splice(at, 1));
      return all;
    }
    case "learn_topic":
      return l.topicView(args.course, args.topic);
    case "learn_review":
      return l.review(args.course, args.topic ?? null);
    case "learn_grade":
      return l.gradeCard(args.course, args.card, args.grade);
    case "learn_concepts":
      return l.concepts(args.course, args.topic);
    case "learn_map":
      return l.map(args.course);
    case "learn_place":
      return l.place(args.course, args.topic, args.step, args.section);
    case "learn_read":
      return l.read(args.course, args.topic);
    case "learn_check":
      return l.check(args.course, args.question, args.answer);
    case "learn_self_grade":
      return l.selfGrade(args.course, args.question, args.knew);
    case "learn_exam":
      return l.exam(args.course, args.scope);
    case "learn_submit":
      return l.submit(args.course, args.scope, args.answers);
    case "learn_ask": {
      const { text: answer, scheme } = splitScheme(await l.ask(args.target, args.text, { voice: Boolean(args.voice) }));
      // Урок с Ноа слушает разговор событиями — так же, как в программе.
      if (args.voice) emit("learn:talk", { q: args.text, a: answer, section: l.walkSection(), scheme });
      return answer;
    }
    case "learn_walk":
      return walkByVoice(l, args.target, args.listen !== false);
    case "learn_walk_stop":
      walking = null;
      stopSpeaking();
      return null;
    case "learn_deep":
      return l.deep(args.course, args.topic, args.section, Boolean(args.cached));
    // Ролики к урокам собирает сервер; здесь — заказ, ход и адрес файла.
    case "learn_videos":
      if (!noa.user) return { videos: [] };
      return site(`/api/noa/videos?course=${encodeURIComponent(args.course)}&topic=${encodeURIComponent(args.topic)}&_=${Date.now()}`);
    case "learn_video_make": {
      if (!noa.user) throw new Error("Ролики собираются на сервере — войдите на сайт.");
      const view = await l.topicView(args.course, args.topic);
      const topic = view.topic ?? view;
      const parts = lessonSections(String(topic.lesson ?? ""));
      const head = `Тема «${topic.title}».`;
      const material =
        args.section == null
          ? `${head} Ролик — обзор всей темы.\n\nКоротко: ${topic.summary ?? ""}\n\nНачало урока:\n${String(topic.lesson ?? "").slice(0, 4000)}`
          : `${head} Ролик — про один раздел урока.\n\nТекст раздела:\n${parts[args.section] ?? ""}`;
      return site("/api/noa/videos", { course: args.course, topic: args.topic, section: args.section ?? null, material, again: Boolean(args.again) });
    }
    case "learn_dramas":
      if (!noa.user) return { dramas: [] };
      return site(`/api/noa/dramas?course=${encodeURIComponent(args.course)}&topic=${encodeURIComponent(args.topic)}&_=${Date.now()}`);
    case "learn_drama_make": {
      if (!noa.user) throw new Error("Аудиодрамы собираются на сервере — войдите на сайт.");
      const view = await l.topicView(args.course, args.topic);
      const topic = view.topic ?? view;
      const settings = args.settings ?? {};
      const body = {
        course: args.course,
        topic: args.topic,
        topicTitle: topic.title,
        scope: args.scope === "section" ? "section" : "lesson",
        level: settings.level,
        junior: settings.junior,
        minutes: settings.minutes,
        again: Boolean(args.again),
      };
      const concepts = (list) => (list ?? []).slice(0, 12).map((c) => `- ${c.term} — ${c.definition}`).join(String.fromCharCode(10));
      if (body.scope === "section") {
        // Сериал — с этой темы и до четырёх следующих.
        const overview = await l.overview();
        const course = (overview ?? []).find((c) => c.id === args.course);
        const from = course?.topics.findIndex((t) => t.id === args.topic) ?? -1;
        const next = from >= 0 ? course.topics.slice(from, from + 5) : [];
        body.lessons = [];
        for (const card of next) {
          const other = await Promise.resolve()
            .then(() => l.topicView(args.course, card.id))
            .catch(() => null);
          const t = other?.topic ?? other;
          if (t) body.lessons.push({ title: t.title, theses: `${t.summary ?? ""}
Понятия:
${concepts(t.concepts)}` });
        }
      } else {
        // Уроки до этого — по ним сервер продолжит сериал курса.
        const overview = await l.overview();
        const course = (overview ?? []).find((c) => c.id === args.course);
        const at = course?.topics.findIndex((t) => t.id === args.topic) ?? -1;
        body.previous = at > 0 ? course.topics.slice(0, at).map((t) => t.id) : [];
        body.material = `Тема «${topic.title}». ${topic.summary ?? ""}

Понятия:
${concepts(topic.concepts)}

Урок:
${String(topic.lesson ?? "").slice(0, 8500)}`;
      }
      return site("/api/noa/dramas", body);
    }
    case "learn_drama_status":
      return site(`/api/noa/dramas/${encodeURIComponent(args.id)}?_=${Date.now()}`);
    case "learn_drama_script":
      return site(`/api/noa/dramas/${encodeURIComponent(args.id)}/script?_=${Date.now()}`);
    case "learn_drama_file":
      // Браузер берёт серию по адресу сам — с перемоткой.
      return `/api/noa/dramas/${encodeURIComponent(args.id)}/episode-${Number(args.episode) || 1}.mp3`;
    case "learn_drama_wanted":
      return null;
    case "learn_video_status":
      return site(`/api/noa/videos/${encodeURIComponent(args.id)}?_=${Date.now()}`);
    case "learn_video_file":
      // Браузер берёт ролик по адресу сам — с докачкой и перемоткой.
      return `/api/noa/videos/${encodeURIComponent(args.id)}/${args.poster ? "poster.jpg" : "video.mp4"}`;
    case "learn_discuss":
      discussByVoice(l, args.target);
      return null;
    case "learn_dictate_start":
      return dictateStart();
    case "learn_dictate_stop":
      return dictateStop();
    case "learn_focus_done":
      return l.focusDone(args.course, args.session);
    // Урок разговором читает вслух тем же голосом, что и остальная Ноа.
    case "voice_speak":
      // Ждём, пока договорит: чтение урока идёт абзац за абзацем.
      await speak(args.text ?? "");
      return null;
    case "voice_busy":
      return speaking();
    case "voice_stop":
      stopSpeaking();
      return null;
    case "learn_focus_bell":
      speak(args.text ?? "");
      return null;
    case "learn_oral":
      if (!canListen) throw new Error("Устный зачёт в этом браузере не работает — сдайте экзамен письменно.");
      oralExam(l, args.course, args.topic ?? null);
      return null;
    case "close_learning":
      location.href = "./";
      return null;
    default:
      throw new Error(`В браузере этого нет: ${cmd}`);
  }
}

/** Тот же вид, что у `tauri()` из bridge.js. */
export const webApi = {
  async invoke(cmd, args) {
    try {
      return await run(cmd, args);
    } catch (err) {
      throw err?.message ?? String(err);
    }
  },
  async listen(event, handler) {
    if (!listeners.has(event)) listeners.set(event, new Set());
    listeners.get(event).add(handler);
    return () => listeners.get(event)?.delete(handler);
  },
};
