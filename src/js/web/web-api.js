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
import { speak, canListen, dictate } from "./voice.js";
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
function discussByVoice(learning, target) {
  startTalk({
    title: "Обсуждение урока",
    greeting: "Слушаю. Что в этом разделе разобрать?",
    reply: (said) => learning.ask(target, said),
    // Сказанное голосом — в ту же ленту обсуждения, что и напечатанное.
    onExchange: (said, answer) => emit("learn:talk", { q: said, a: answer }),
  });
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
  dictionary = new WebHost({ client: dictionaryClient(), requireLeftCtrl: true }).mount();
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
  link.href = "./";
  link.textContent = "Подключить модель";
  bar.append("Модель не подключена — проверка ответов, обсуждение и разбор не заработают. ", link);
  document.body.prepend(bar);
}

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
    case "learn_ask":
      return l.ask(args.target, args.text);
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
