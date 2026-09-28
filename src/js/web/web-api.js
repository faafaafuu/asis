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
import { dictionaryClient } from "./ai-web.js";
import { WebHost } from "../web-host.js";

const listeners = new Map();

function emit(event, payload) {
  for (const handler of listeners.get(event) ?? []) handler({ event, payload });
}

/* ── Голос браузера ─────────────────────────────────────────────────────── */

const Recognition = globalThis.SpeechRecognition ?? globalThis.webkitSpeechRecognition;

/** Текст для чтения вслух: без разметки и кода. */
const plain = (text) =>
  String(text)
    .replace(/```[\s\S]*?```/g, " ")
    .replace(/[`*_#>]/g, "")
    .replace(/\s+/g, " ")
    .trim();

function speak(text) {
  return new Promise((resolve) => {
    if (!globalThis.speechSynthesis) return resolve();
    const phrase = new SpeechSynthesisUtterance(plain(text));
    phrase.lang = "ru-RU";
    const voice = speechSynthesis.getVoices().find((v) => v.lang?.startsWith("ru"));
    if (voice) phrase.voice = voice;
    phrase.onend = phrase.onerror = () => resolve();
    speechSynthesis.cancel();
    speechSynthesis.speak(phrase);
  });
}

/** Одна фраза с микрофона. Пусто — не расслышал. */
function hearOnce() {
  return new Promise((resolve, reject) => {
    if (!Recognition) return reject(new Error("Голос в этом браузере не работает — откройте Ноа в Chrome или Edge."));
    const ear = new Recognition();
    ear.lang = "ru-RU";
    ear.interimResults = false;
    let said = "";
    ear.onresult = (event) => (said = event.results[0]?.[0]?.transcript ?? "");
    ear.onerror = (event) =>
      event.error === "not-allowed" ? reject(new Error("Браузер не дал микрофон — разрешите его в адресной строке.")) : resolve("");
    ear.onend = () => resolve(said.trim());
    ear.start();
  });
}

/** Диктовка ответа: запись идёт, пока не нажали «Готово». */
let dictation = null;

function dictateStart() {
  if (!Recognition) throw new Error("Диктовка в этом браузере не работает — откройте Ноа в Chrome или Edge.");
  const ear = new Recognition();
  ear.lang = "ru-RU";
  ear.continuous = true;
  ear.interimResults = false;
  const parts = [];
  const done = new Promise((resolve) => (ear.onend = resolve));
  ear.onresult = (event) => {
    for (let at = event.resultIndex; at < event.results.length; at++) {
      if (event.results[at].isFinal) parts.push(event.results[at][0].transcript);
    }
  };
  ear.onerror = () => {};
  ear.start();
  dictation = { ear, parts, done };
}

async function dictateStop() {
  if (!dictation) throw new Error("Запись не шла.");
  const { ear, parts, done } = dictation;
  dictation = null;
  ear.stop();
  await done;
  const text = parts.join(" ").trim();
  if (!text) throw new Error("Ничего не расслышал — попробуйте ещё раз.");
  return text;
}

/** Обсуждение голосом: Ноа отвечает вслух и слушает дальше, пока не скажут «спасибо». */
let talking = false;

async function discussByVoice(learning, target) {
  if (!Recognition) throw new Error("Голос в этом браузере не работает — спросите текстом или откройте Ноа в Chrome.");
  if (talking) return;
  talking = true;
  try {
    await speak("Слушаю. Что разобрать?");
    for (let quiet = 0; talking && quiet < 2; ) {
      const said = await hearOnce();
      if (!said) {
        quiet++;
        continue;
      }
      quiet = 0;
      if (/\b(спасибо|хватит|стоп|пока)\b/i.test(said)) {
        await speak("Хорошо.");
        break;
      }
      const reply = await learning.ask(target, said).catch((err) => `Не получилось ответить: ${err.message}`);
      emit("learn:talk", { q: said, a: reply });
      await speak(reply);
    }
  } finally {
    talking = false;
  }
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

/** Курс, открытый по ссылке `?course=`, — первым: окно берёт первый. */
const wanted = new URLSearchParams(location.search).get("course");

async function run(cmd, args = {}) {
  const noa = await openNoa();
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
      discussByVoice(l, args.target).catch((err) => emit("learn:talk", { q: "🎙", a: err.message }));
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
      throw new Error("Устный зачёт пока только в программе Ноа — здесь сдайте экзамен письменно.");
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
