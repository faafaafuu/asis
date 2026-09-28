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
import { speak, listen, canListen, stopSpeaking } from "./voice.js";

const listeners = new Map();

function emit(event, payload) {
  for (const handler of listeners.get(event) ?? []) handler({ event, payload });
}

/* ── Голос ──────────────────────────────────────────────────────────────── */

const Recognition = globalThis.SpeechRecognition ?? globalThis.webkitSpeechRecognition;

/** «Спасибо», «хватит» — закончить разговор. Слова целиком, без : он не видит кириллицу. */
const BYE = /(^|[^\p{L}])(спасибо|хватит|стоп|пока|закончим)([^\p{L}]|$)/iu;

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
  if (!canListen) throw new Error("Голос в этом браузере не работает — спросите текстом или откройте Ноа в Chrome.");
  if (talking) return;
  talking = true;
  try {
    await speak("Слушаю. Что разобрать?");
    for (let quiet = 0; talking && quiet < 3; ) {
      const said = await listen();
      if (!said) {
        quiet++;
        continue;
      }
      quiet = 0;
      if (BYE.test(said) && said.split(/\s+/).length <= 4) {
        await speak("Хорошо, закончили.");
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

/* ── Устный зачёт ───────────────────────────────────────────────────────── */

/**
 * Панель зачёта в углу страницы: что спросила Ноа, что она услышала и кнопка
 * «Закончить». Голос уходит из виду, и без неё непонятно, расслышала ли Ноа
 * ответ и идёт ли зачёт вообще.
 */
function oralPanel(onStop) {
  document.querySelector(".oral")?.remove();
  const panel = document.createElement("section");
  panel.className = "oral";
  panel.setAttribute("aria-live", "polite");
  const head = document.createElement("div");
  head.className = "oral__head";
  const title = document.createElement("strong");
  title.textContent = "Устный зачёт";
  const state = document.createElement("span");
  state.className = "oral__state";
  const stop = document.createElement("button");
  stop.type = "button";
  stop.className = "button button--quiet";
  stop.textContent = "Закончить";
  stop.addEventListener("click", onStop);
  head.append(title, state, stop);
  const log = document.createElement("div");
  log.className = "oral__log";
  panel.append(head, log);
  document.body.append(panel);
  return {
    say(who, text) {
      const line = document.createElement("p");
      line.className = `oral__line oral__line--${who}`;
      line.textContent = text;
      log.append(line);
      log.scrollTop = log.scrollHeight;
    },
    state: (text) => (state.textContent = text),
    close: () => setTimeout(() => panel.remove(), 8000),
  };
}

let oral = false;

async function oralExam(learning, courseId, topicId) {
  if (!canListen) throw new Error("Устный зачёт в этом браузере не работает — откройте Ноа в Chrome или Edge.");
  if (oral) return;
  oral = true;
  const panel = oralPanel(() => {
    oral = false;
    stopSpeaking();
  });
  try {
    let text = learning.oralStart(courseId, topicId);
    let done = !learning.oralActive();
    for (let quiet = 0; oral; ) {
      panel.say("noa", text);
      panel.state("говорит");
      await speak(text);
      if (done || !oral) break;
      panel.state("слушаю…");
      const said = await listen({ onHeard: (text) => panel.state(`слышу: ${text.slice(-60)}`) });
      if (!oral) break;
      if (!said) {
        if (++quiet >= 3) {
          text = learning.oralStop() ?? "Закончили.";
          done = true;
          continue;
        }
        text = "Не расслышала. Повторите ответ или скажите «не знаю».";
        continue;
      }
      quiet = 0;
      panel.say("me", said);
      panel.state("проверяю…");
      ({ text, done } = await learning.oralAnswer(said));
    }
  } catch (err) {
    panel.say("error", err.message);
  } finally {
    if (learning.oralActive()) learning.oralStop();
    oral = false;
    panel.state("закончен");
    panel.close();
    emit("learn:changed", {});
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
      if (!canListen) throw new Error("Устный зачёт в этом браузере не работает — откройте Ноа в Chrome или Edge.");
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
