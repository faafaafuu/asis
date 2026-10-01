// Урок с Ноа — урок как живой разговор с репетитором, от первого раздела до
// последнего.
//
// Ноа держит урок в уме и ведёт по нему: рассказывает раздел своими словами —
// что это, зачем на практике, как об этом спросят на собеседовании, — и
// просит пересказать, как вы поняли. Пересказ она разбирает: что верно, что
// упустили или перепутали. Поняли — сама переходит к следующему разделу; нет
// — объясняет иначе и спрашивает снова. Можно перебивать вопросами.
//
// Разговор голосом — без кнопок: Ноа говорит, потом слушает, вы отвечаете,
// она отвечает. Можно и текстом. Ведение урока живёт там же, где модель:
// tutor.rs в программе, learn-core.js в браузере. Окно только показывает
// разговор — он приходит событием `learn:talk`.

import { parseScheme, renderScheme } from "./scheme.js";

/** Заголовок раздела «## …» — для подписи. */
export function sectionTitle(part) {
  return /^##\s+(.+)$/m.exec(String(part ?? ""))?.[1]?.trim() ?? "";
}

/** Признак конца урока в реплике Ноа. */
export const DONE = "Урок пройден";

/**
 * Разговор урока переживает переход на «Материал» и обратно, и даже
 * перезагрузку: строки лежат в памяти браузера по курсу и теме.
 */
const LOG_KEY = (course, topic) => `noa.agent.${course}/${topic}`;
const MAX_LINES = 80;

function loadLog(key) {
  try {
    const saved = JSON.parse(localStorage.getItem(key) ?? "null");
    return Array.isArray(saved?.lines) ? saved : null;
  } catch {
    return null;
  }
}

function saveLog(key, data) {
  try {
    localStorage.setItem(key, JSON.stringify({ ...data, lines: data.lines.slice(-MAX_LINES) }));
  } catch {
    /* не запомнится — не беда */
  }
}

/**
 * Один слушатель разговора на страницу. Окно урока перерисовывается при
 * каждом переходе между вкладками; слушатель же живёт всё время и пишет
 * реплики в память темы — и пока открыт «Материал», голос продолжается, а
 * вернувшись, вы видите весь разговор.
 */
const live = { key: null, parts: [], view: null, subscribed: false };

function subscribe(api) {
  if (live.subscribed) return;
  live.subscribed = true;
  api.listen?.("learn:talk", (event) => {
    if (!live.key) return;
    const { q, a, section, scheme } = event.payload ?? {};
    const data = loadLog(live.key) ?? { at: 0, lines: [] };
    const add = [];
    if (q) add.push({ who: "me", text: q });
    if (Number.isInteger(section) && section !== data.at) {
      data.at = section;
      const title = sectionTitle(live.parts[section]);
      add.push({ who: "mark", text: `— Раздел ${section + 1}${title ? `: ${title}` : ""} —` });
    }
    if (a) add.push({ who: "noa", text: a });
    // Схема раздела — сразу под рассказом: смотреть, пока Ноа говорит.
    if (scheme) add.push({ who: "scheme", text: typeof scheme === "string" ? scheme : JSON.stringify(scheme) });
    data.lines.push(...add);
    saveLog(live.key, data);
    live.view?.(add, data.at, a);
  });
  api.listen?.("learn:listening", (event) => live.listening?.(Boolean(event.payload)));
}

/**
 * Рисует урок с Ноа в `root`.
 * deps: { api, course, topic, parts, start, el, button, markdown, openStep }
 */
export function renderLessonAgent(root, deps) {
  const { api, course, topic, parts, el, button, markdown } = deps;
  const total = parts.length;
  const key = LOG_KEY(course.id, topic.id);
  const saved = loadLog(key);
  const state = {
    at: Math.min(saved?.at ?? deps.start ?? 0, total - 1),
    // Голос после перехода не продолжается сам: продолжить — одной кнопкой.
    mode: saved?.lines.length ? "text" : "idle",
    busy: false,
  };
  const lines = saved?.lines ?? [];

  const box = el("section", "agent");
  const head = el("div", "agent__head");
  const where = el("span", "agent__where");
  const status = el("span", "agent__status");
  head.append(where, status);

  const log = el("div", "agent__log");
  const startRow = el("div", "actions agent__start");
  const voiceStart = button("🎙 Начать урок голосом", () => start(true));
  const textStart = button("Текстом", () => start(false), true);
  startRow.append(voiceStart, textStart);
  startRow.prepend(el("p", "note agent__intro", "Ноа рассказывает раздел своими словами и просит пересказать, как вы поняли. Голосом — просто говорите, когда она замолчит."));

  const area = el("textarea", "answer agent__input");
  area.rows = 2;
  area.placeholder = "Ответить текстом — Enter";
  const send = button("Отправить", () => reply());
  // Замолчать — сразу, в любой момент. В разговоре голосом Ноа после этого
  // слушает: можно перебить и спросить.
  const hush = button("⏸ Замолчать", () => api.invoke("voice_stop").catch(() => {}), true);
  hush.title = "Замолчать · Esc";
  const stop = button("⏹ Закончить голосом", () => stopVoice(), true);
  const voiceAgain = button("🎙 Продолжить голосом", () => start(true), true);
  let aloud = readAloud();
  const aloudBtn = button("", () => {
    aloud = !aloud;
    saveAloud(aloud);
    if (!aloud) api.invoke("voice_stop").catch(() => {});
    paint();
  }, true);
  const actions = el("div", "actions agent__actions");
  actions.append(send, hush, stop, voiceAgain, aloudBtn);
  const finish = el("div", "actions agent__finish");
  box.append(head, log, startRow, area, actions, finish);
  root.append(box);

  // Начали печатать — Ноа замолкает: вас слушают, а не перебивают.
  area.addEventListener("input", () => api.invoke("voice_stop").catch(() => {}), { once: false });
  const esc = (event) => {
    if (!box.isConnected) return document.removeEventListener("keydown", esc);
    if (event.key === "Escape") api.invoke("voice_stop").catch(() => {});
  };
  document.addEventListener("keydown", esc);
  area.addEventListener("keydown", (event) => {
    if (event.key === "Enter" && !event.shiftKey) {
      event.preventDefault();
      reply();
    }
  });

  function paint() {
    const title = sectionTitle(parts[state.at]);
    where.textContent = `Раздел ${state.at + 1} из ${total}${title ? ` · ${title}` : ""}`;
    startRow.hidden = state.mode !== "idle";
    area.hidden = state.mode === "idle";
    actions.hidden = state.mode === "idle";
    stop.hidden = state.mode !== "voice";
    voiceAgain.hidden = state.mode !== "text";
    aloudBtn.hidden = state.mode !== "text";
    aloudBtn.textContent = aloud ? "🔊 Вслух" : "🔇 Без голоса";
    send.disabled = state.busy;
  }

  function line(who, text, keep = true) {
    if (keep && who !== "wait") {
      lines.push({ who, text });
      saveLog(key, { at: state.at, lines });
    }
    const node = el("div", `agent__line agent__line--${who}`);
    if (who === "scheme") {
      const scheme = parseScheme(text);
      if (!scheme) return node;
      node.append(renderScheme(scheme));
    } else if (who === "noa") node.innerHTML = markdown(text);
    else node.textContent = text;
    log.append(node);
    node.scrollIntoView({ block: "nearest", behavior: "smooth" });
    return node;
  }

  const target = () => ({ course: course.id, topic: topic.id, section: state.at });

  // Разговор приходит событиями — общий слушатель пишет его в память темы,
  // а сюда отдаёт новые строки, пока это окно на экране.
  let wait = null;
  subscribe(api);
  live.key = key;
  live.parts = parts;
  live.view = (added, at, a) => {
    if (!box.isConnected) return;
    wait?.remove();
    wait = null;
    state.at = at;
    for (const x of added) {
      lines.push(x);
      line(x.who, x.text, false);
    }
    if (state.mode === "text" && aloud && a) api.invoke("voice_speak", { text: a }).catch(() => {});
    if (a?.includes(DONE)) finished();
    paint();
  };
  live.listening = (on) => {
    if (box.isConnected) status.textContent = on ? "🎙 Слушаю…" : "";
  };

  async function start(voice) {
    state.mode = voice ? "voice" : "text";
    state.busy = true;
    paint();
    if (!log.childElementCount) {
      line("mark", `— Раздел ${state.at + 1}${sectionTitle(parts[state.at]) ? `: ${sectionTitle(parts[state.at])}` : ""} —`);
    }
    wait = line("wait", "Ноа готовится…");
    try {
      await api.invoke("learn_walk", { target: target(), listen: voice });
    } catch (err) {
      wait?.remove();
      wait = null;
      line("error", `Не получилось начать: ${err}`);
      state.mode = "idle";
    } finally {
      state.busy = false;
      paint();
    }
  }

  async function reply() {
    const said = area.value.trim();
    if (!said || state.busy) return;
    if (state.mode === "voice") stopVoice();
    api.invoke("voice_stop").catch(() => {});
    area.value = "";
    state.busy = true;
    paint();
    wait = line("wait", "Ноа думает…");
    try {
      await api.invoke("learn_ask", { target: target(), text: said, voice: true });
    } catch (err) {
      wait?.remove();
      wait = null;
      line("error", `Ответа нет: ${err}`);
    } finally {
      state.busy = false;
      paint();
    }
  }

  function stopVoice() {
    api.invoke("learn_walk_stop").catch(() => {});
    state.mode = "text";
    status.textContent = "";
    paint();
  }

  function finished() {
    finish.replaceChildren();
    for (const [key, label] of [
      ["sheet", "📝 Конспект"],
      ["review", "🃏 Карточки"],
      ["tasks", "Проверить себя"],
    ]) {
      finish.append(button(label, () => deps.openStep?.(key), key !== "sheet"));
    }
  }

  // Прежний разговор этой темы — на место.
  for (const { who, text } of lines) line(who, text, false);
  if (lines.some((x) => x.who === "noa" && x.text.includes(DONE))) finished();
  if (state.mode === "text") {
    const clear = button("Начать урок заново", () => {
      lines.length = 0;
      saveLog(key, { at: 0, lines });
      log.replaceChildren();
      state.at = 0;
      state.mode = "idle";
      finish.replaceChildren();
      clear.remove();
      paint();
    }, true);
    actions.append(clear);
  }
  paint();
}

const ALOUD_KEY = "noa.agentAloud";

function readAloud() {
  try {
    return localStorage.getItem(ALOUD_KEY) !== "off";
  } catch {
    return true;
  }
}

function saveAloud(on) {
  try {
    localStorage.setItem(ALOUD_KEY, on ? "on" : "off");
  } catch {
    /* не запомнится */
  }
}
