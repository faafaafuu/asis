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
import { dictionaryClient, loadModel, saveModel, bridgeAvailable } from "./ai-web.js";

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
  const first = await learning.walkStart(target);
  emit("learn:talk", { q: "", a: first, section: learning.walkSection() });
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
      const said = String((await listen().catch(() => "")) ?? "").trim();
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
        text = await learning.ask({ ...target, section: learning.walkSection() ?? target.section }, said, { voice: true });
        emit("learn:talk", { q: said, a: text, section: learning.walkSection() });
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

/** Курс, открытый по ссылке `?course=`, — первым: окно берёт первый. */
const wanted = new URLSearchParams(location.search).get("course");

async function run(cmd, args = {}) {
  const noa = await openNoa();
  ensureModel(noa.user);
  const l = noa.learning;
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
      const answer = await l.ask(args.target, args.text, { voice: Boolean(args.voice) });
      // Урок с Ноа слушает разговор событиями — так же, как в программе.
      if (args.voice) emit("learn:talk", { q: args.text, a: answer, section: l.walkSection() });
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
