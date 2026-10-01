// Урок с Ноа — урок как живой разговор с репетитором, от первого раздела до
// последнего.
//
// Ноа держит урок в уме и ведёт по нему: рассказывает раздел своими словами —
// что это, зачем на практике, как об этом спросят на собеседовании, — и
// просит пересказать, как вы поняли. Пересказ она разбирает. Поняли — сама
// переходит к следующему разделу; нет — объясняет иначе и спрашивает снова.
//
// На экране — не стена текста, а самое важное: кольцо Ноа (говорит, слушает,
// думает), её вопрос, план урока с подсвеченным разделом и у него — главное
// пунктами и схема. Весь текст разговора — свёрнут, по желанию.
//
// Ведение урока живёт там же, где модель: tutor.rs в программе,
// learn-core.js в браузере. Окно только показывает разговор — он приходит
// событием `learn:talk`.

import { parseScheme, renderScheme } from "./scheme.js";
import { createOrb } from "./orb.js";

/** Заголовок раздела «## …» — для плана. */
export function sectionTitle(part) {
  return /^##\s+(.+)$/m.exec(String(part ?? ""))?.[1]?.trim() ?? "";
}

/** Признак конца урока в реплике Ноа. */
export const DONE = "Урок пройден";

/** Последний вопрос в реплике Ноа — его и показываем крупно. */
export function lastQuestion(text) {
  const sentences = String(text ?? "").replace(/\s+/g, " ").match(/[^.!?…]+[?]/g) ?? [];
  return sentences.length ? sentences[sentences.length - 1].trim() : "";
}

/**
 * Разговор урока переживает переход на «Материал» и обратно и перезагрузку:
 * лежит в памяти браузера по курсу и теме — реплики, раздел, схемы разделов.
 */
const LOG_KEY = (course, topic) => `noa.agent.${course}/${topic}`;
const MAX_LINES = 80;

function loadLog(key) {
  try {
    const saved = JSON.parse(localStorage.getItem(key) ?? "null");
    return Array.isArray(saved?.lines) ? { sections: {}, ...saved } : null;
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
 * Один слушатель разговора на страницу: окно перерисовывается при каждом
 * переходе между вкладками, а слушатель пишет всё в память темы — и пока
 * открыт «Материал», голос продолжается.
 */
const live = { key: null, view: null, subscribed: false };

function subscribe(api) {
  if (live.subscribed) return;
  live.subscribed = true;
  api.listen?.("learn:talk", (event) => {
    if (!live.key) return;
    const { q, a, section, scheme } = event.payload ?? {};
    const data = loadLog(live.key) ?? { at: 0, lines: [], sections: {} };
    if (Number.isInteger(section)) data.at = section;
    if (q) data.lines.push({ who: "me", text: q });
    if (a) data.lines.push({ who: "noa", text: a });
    if (scheme) data.sections[data.at] = { scheme: typeof scheme === "string" ? scheme : JSON.stringify(scheme) };
    if (a?.includes(DONE)) data.done = true;
    saveLog(live.key, data);
    live.view?.(data);
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
  let data = loadLog(key) ?? { at: Math.min(deps.start ?? 0, total - 1), lines: [], sections: {} };
  const state = { mode: data.lines.length ? "text" : "idle", waiting: false, listening: false };

  const box = el("section", "agent");

  // Сцена: кольцо Ноа, что она делает и её вопрос.
  const stage = el("div", "agent__stage");
  const canvas = el("canvas", "agent__orb");
  canvas.width = 360;
  canvas.height = 180;
  const orb = createOrb(canvas);
  const now = el("div", "agent__now");
  const status = el("span", "agent__status");
  const where = el("span", "agent__where");
  const ask = el("p", "agent__ask");
  now.append(status, where, ask);
  stage.append(canvas, now);

  // Управление — коротко.
  const controls = el("div", "agent__controls");
  const voiceStart = button("🎙 Начать голосом", () => start(true));
  const textStart = button("Текстом", () => start(false), true);
  const hush = button("⏸ Замолчать", () => api.invoke("voice_stop").catch(() => {}), true);
  hush.title = "Замолчать · Esc";
  const stop = button("⏹ Закончить", () => stopVoice(), true);
  const typeBtn = button("⌨ Ответить текстом", () => toggleType(), true);
  controls.append(voiceStart, textStart, hush, stop, typeBtn);

  const typeRow = el("div", "agent__type");
  const area = el("textarea", "answer agent__input");
  area.rows = 2;
  area.placeholder = "Перескажите своими словами или спросите — Enter";
  const send = button("Отправить", () => reply());
  typeRow.append(area, send);
  typeRow.hidden = true;

  // План урока: разделы, текущий раскрыт — главное и схема.
  const plan = el("ol", "agent__plan");

  const transcript = el("details", "agent__transcript");
  transcript.append(el("summary", "", "Текст разговора"));
  const log = el("div", "agent__log");
  transcript.append(log);

  const finish = el("div", "actions agent__finish");
  box.append(stage, controls, typeRow, plan, finish, transcript);
  root.append(box);

  function paintPlan() {
    plan.replaceChildren();
    parts.forEach((part, i) => {
      const item = el("li", "agent__step");
      const passed = i < data.at || (data.done && i <= data.at);
      const current = i === data.at && !data.done;
      if (passed) item.classList.add("is-done");
      if (current) item.classList.add("is-now");
      const head = el("div", "agent__step-head");
      head.append(
        el("span", "agent__step-num", passed ? "✓" : String(i + 1)),
        el("span", "agent__step-title", sectionTitle(part) || (i === 0 ? "Вступление" : `Раздел ${i + 1}`)),
      );
      item.append(head);
      const saved = data.sections?.[i]?.scheme ? parseScheme(data.sections[i].scheme) : null;
      if (saved && current) {
        if (saved.points.length) {
          const list = el("ul", "agent__points");
          for (const point of saved.points) list.append(el("li", "", point));
          item.append(list);
        }
        if (saved.nodes.length >= 2) item.append(renderScheme(saved));
      } else if (saved && passed && saved.points[0]) {
        // Пройденный раздел — свёрнуто: одна строка главного.
        item.append(el("p", "agent__step-gist", saved.points[0]));
      }
      plan.append(item);
    });
    plan.querySelector(".is-now")?.scrollIntoView({ block: "nearest", behavior: "smooth" });
  }

  function paintLog() {
    log.replaceChildren(
      ...data.lines.slice(-40).map(({ who, text }) => {
        const node = el("div", `agent__line agent__line--${who}`);
        if (who === "noa") node.innerHTML = markdown(text);
        else node.textContent = text;
        return node;
      }),
    );
  }

  function paint() {
    const title = sectionTitle(parts[data.at]);
    where.textContent = data.done ? "Урок пройден" : `Раздел ${data.at + 1} из ${total}${title ? ` · ${title}` : ""}`;
    const lastNoa = [...data.lines].reverse().find((x) => x.who === "noa")?.text ?? "";
    ask.textContent = data.done ? "" : lastQuestion(lastNoa);
    voiceStart.hidden = state.mode === "voice" || data.done;
    voiceStart.textContent = data.lines.length ? "🎙 Продолжить голосом" : "🎙 Начать голосом";
    textStart.hidden = state.mode !== "idle" || data.lines.length > 0;
    stop.hidden = state.mode !== "voice";
    hush.hidden = state.mode === "idle";
    typeBtn.hidden = state.mode === "idle" || data.done;
  }

  // Кольцо и подпись — по тому, что происходит: говорит, слушает, думает.
  const tick = setInterval(async () => {
    if (!box.isConnected) {
      clearInterval(tick);
      orb.stop();
      return;
    }
    const speaking = Boolean(await api.invoke("voice_busy").catch(() => false));
    const mode = speaking ? "speaking" : state.waiting ? "thinking" : state.mode === "voice" ? "listening" : "idle";
    orb.setMode(mode);
    status.textContent = {
      speaking: "Ноа рассказывает",
      thinking: "Ноа думает…",
      listening: "Слушаю — говорите",
      idle: state.mode === "idle" ? "Готова начать урок" : "Ваша очередь",
    }[mode];
  }, 400);

  subscribe(api);
  live.key = key;
  live.view = (fresh) => {
    if (!box.isConnected) return;
    data = fresh;
    state.waiting = false;
    const lastNoa = [...data.lines].reverse().find((x) => x.who === "noa")?.text;
    if (state.mode === "text" && aloud() && lastNoa) api.invoke("voice_speak", { text: lastNoa }).catch(() => {});
    if (data.done) finished();
    paint();
    paintPlan();
    paintLog();
  };
  live.listening = (on) => {
    state.listening = on;
  };

  const target = () => ({ course: course.id, topic: topic.id, section: data.at });

  async function start(voice) {
    state.mode = voice ? "voice" : "text";
    state.waiting = true;
    if (!voice) typeRow.hidden = false;
    paint();
    try {
      await api.invoke("learn_walk", { target: target(), listen: voice });
    } catch (err) {
      state.waiting = false;
      state.mode = data.lines.length ? "text" : "idle";
      paint();
      status.textContent = `Не получилось начать: ${err}`;
    }
  }

  async function reply() {
    const said = area.value.trim();
    if (!said || state.waiting) return;
    if (state.mode === "voice") stopVoice();
    api.invoke("voice_stop").catch(() => {});
    area.value = "";
    state.waiting = true;
    try {
      await api.invoke("learn_ask", { target: target(), text: said, voice: true });
    } catch (err) {
      state.waiting = false;
      status.textContent = `Ответа нет: ${err}`;
    }
  }

  function toggleType() {
    typeRow.hidden = !typeRow.hidden;
    if (!typeRow.hidden) area.focus();
  }

  function stopVoice() {
    api.invoke("learn_walk_stop").catch(() => {});
    state.mode = "text";
    paint();
  }

  function finished() {
    finish.replaceChildren();
    for (const [step, label] of [
      ["sheet", "📝 Конспект"],
      ["review", "🃏 Карточки"],
      ["tasks", "Проверить себя"],
    ]) {
      finish.append(button(label, () => deps.openStep?.(step), step !== "sheet"));
    }
  }

  // Начали печатать — Ноа замолкает; Esc — тоже.
  area.addEventListener("input", () => api.invoke("voice_stop").catch(() => {}));
  area.addEventListener("keydown", (event) => {
    if (event.key === "Enter" && !event.shiftKey) {
      event.preventDefault();
      reply();
    }
  });
  const esc = (event) => {
    if (!box.isConnected) return document.removeEventListener("keydown", esc);
    if (event.key === "Escape") api.invoke("voice_stop").catch(() => {});
  };
  document.addEventListener("keydown", esc);

  if (data.lines.length) {
    const again = button("↺ Заново", () => {
      data = { at: 0, lines: [], sections: {} };
      saveLog(key, data);
      state.mode = "idle";
      finish.replaceChildren();
      again.remove();
      paint();
      paintPlan();
      paintLog();
    }, true);
    again.title = "Начать урок заново";
    controls.append(again);
  }
  if (data.done) finished();
  paint();
  paintPlan();
  paintLog();
}

const ALOUD_KEY = "noa.agentAloud";
const aloud = () => {
  try {
    return localStorage.getItem(ALOUD_KEY) !== "off";
  } catch {
    return true;
  }
};
