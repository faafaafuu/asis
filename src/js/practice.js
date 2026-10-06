// Окно практики в терминале.
//
// Слева — настоящий терминал (xterm.js поверх псевдотерминала в practice.rs):
// человек подключается к своему серверу и работает руками. Справа — Ноа:
// текущий шаг сценария, общая картина с подсвеченным местом, где сейчас
// работа, и лента её замечаний. Замечания пишет программа — она видит вывод
// терминала и сама решает, когда посмотреть (команда кончилась или идёт
// долго); окно только показывает и читает их вслух.

import { tauri, appWindow, applyTheme } from "./bridge.js";
import { Terminal } from "../vendor/xterm/xterm.js";
import { FitAddon } from "../vendor/xterm/addon-fit.js";
import { parseScheme, renderScheme } from "./scheme.js";
import { createOrb } from "./orb.js";

const api = tauri();
const ui = {};
for (const node of document.querySelectorAll("[data-el]")) ui[node.dataset.el] = node;

function el(tag, className = "", text = "") {
  const node = document.createElement(tag);
  if (className) node.className = className;
  if (text) node.textContent = text;
  if (tag === "button") node.type = "button";
  return node;
}

function button(text, onClick, quiet = false) {
  const node = el("button", quiet ? "button button--quiet" : "button", text);
  node.addEventListener("click", onClick);
  return node;
}

const escapeHtml = (text) =>
  String(text).replace(/[&<>"']/g, (ch) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[ch]);

/** Реплика Ноа: `команды` — кодом, **важное** — жирным, остальное как есть. */
function inline(text) {
  return escapeHtml(text)
    .replace(/`([^`]+)`/g, "<code>$1</code>")
    .replace(/\*\*([^*]+)\*\*/g, "<b>$1</b>")
    .replace(/\n/g, "<br>");
}

/** Читать ли замечания вслух — помнится между запусками. */
const ALOUD = "noa.practiceAloud";
const aloud = () => {
  try {
    return localStorage.getItem(ALOUD) !== "off";
  } catch {
    return true;
  }
};

let practice = null;
let waiting = false;
let listening = false;
let error = "";

/* ── Окно ──────────────────────────────────────────────────────────────── */

const win = appWindow();
ui.minimize.addEventListener("click", () => win?.minimize());
ui.close.addEventListener("click", () => {
  api?.invoke("voice_stop").catch(() => {});
  win?.close();
});
ui.head.addEventListener("pointerdown", (event) => {
  if (event.button !== 0 || event.target.closest("button")) return;
  event.preventDefault();
  win?.startDragging();
});
api?.invoke("runtime_config").then((config) => applyTheme(config?.theme)).catch(() => {});
api?.invoke("app_version")
  .then((version) => (ui.appVersion.textContent = `NOAH ${version}`))
  .catch(() => {});

/* ── Терминал ──────────────────────────────────────────────────────────── */

const mono = getComputedStyle(document.documentElement).getPropertyValue("--font-mono").trim();
const term = new Terminal({
  fontFamily: `${mono ? `${mono}, ` : ""}"Cascadia Mono", Consolas, monospace`,
  fontSize: 14,
  lineHeight: 1.15,
  cursorBlink: true,
  scrollback: 5000,
  theme: {
    background: "#16130f",
    foreground: "#f2ece1",
    cursor: "#f2c14e",
    selectionBackground: "#3d5a99",
  },
});
const fit = new FitAddon();
term.loadAddon(fit);
let exited = false;

async function startShell() {
  exited = false;
  try {
    const replay = await api.invoke("practice_term_start", { cols: term.cols, rows: term.rows });
    if (replay) term.write(replay);
  } catch (err) {
    term.write(`\r\n\x1b[31mТерминал не запустился: ${err}\x1b[0m\r\n`);
  }
}

// Копировать — Ctrl+C при выделении (без выделения это «прервать»),
// вставить — Ctrl+V и правый клик, как в Windows Terminal.
async function paste() {
  const text = await api.invoke("practice_clipboard").catch(() => null);
  if (text) term.paste(text);
}

term.attachCustomKeyEventHandler((event) => {
  if (event.type !== "keydown") return true;
  const key = event.key.toLowerCase();
  if (event.ctrlKey && key === "c" && term.hasSelection()) {
    navigator.clipboard?.writeText(term.getSelection()).catch(() => {});
    term.clearSelection();
    return false;
  }
  if (event.ctrlKey && key === "v") {
    event.preventDefault();
    paste();
    return false;
  }
  return true;
});

ui.screen.addEventListener("contextmenu", (event) => {
  event.preventDefault();
  if (term.hasSelection()) {
    navigator.clipboard?.writeText(term.getSelection()).catch(() => {});
    term.clearSelection();
  } else paste();
});

term.onData((data) => {
  if (exited) {
    if (data.includes("\r")) {
      term.reset();
      startShell();
    }
    return;
  }
  api.invoke("practice_term_write", { data }).catch(() => {});
});

api?.listen("practice:out", (event) => term.write(event.payload));
api?.listen("practice:exit", () => {
  exited = true;
  term.write("\r\n\x1b[2m[оболочка закрылась — Enter, чтобы открыть новую]\x1b[0m\r\n");
});

let resizeTimer = 0;
new ResizeObserver(() => {
  clearTimeout(resizeTimer);
  resizeTimer = setTimeout(() => {
    fit.fit();
    api.invoke("practice_term_resize", { cols: term.cols, rows: term.rows }).catch(() => {});
  }, 60);
}).observe(ui.screen);

/* ── Ноа ───────────────────────────────────────────────────────────────── */

// Кольцо и подписи живут дольше одной отрисовки панели — создаются один раз.
const canvas = el("canvas", "coach__orb");
canvas.width = 240;
canvas.height = 120;
const orb = createOrb(canvas);
const status = el("span", "coach__status");
const sub = el("span", "coach__sub");

function stage() {
  const box = el("div", "coach__stage");
  const now = el("div", "coach__now");
  now.append(status, sub);
  const toggles = el("div", "coach__toggles");
  const eye = el("button", "icon-toggle", "👁");
  eye.title = practice?.watching ? "Ноа смотрит терминал — выключить" : "Ноа не смотрит — включить";
  eye.setAttribute("aria-pressed", String(Boolean(practice?.watching)));
  eye.addEventListener("click", () => api.invoke("practice_watch", { on: !practice?.watching }).catch(() => {}));
  const voice = el("button", "icon-toggle", "🔊");
  voice.title = aloud() ? "Замечания вслух — выключить" : "Замечания молча — включить голос";
  voice.setAttribute("aria-pressed", String(aloud()));
  voice.addEventListener("click", () => {
    try {
      localStorage.setItem(ALOUD, aloud() ? "off" : "on");
    } catch {
      /* не запомнится — не беда */
    }
    if (!aloud()) api.invoke("voice_stop").catch(() => {});
    render();
  });
  // 💬 — разбирать каждую команду. Выключено — только ошибки и просьбы:
  // каждая команда — запрос к модели.
  const verbose = el("button", "icon-toggle", "💬");
  verbose.title = practice?.verbose
    ? "Разбирает каждую команду — выключить, говорить только об ошибках и по просьбе"
    : "Говорит только об ошибках и по просьбе — включить разбор каждой команды";
  verbose.setAttribute("aria-pressed", String(Boolean(practice?.verbose)));
  verbose.addEventListener("click", () => api.invoke("practice_verbose", { on: !practice?.verbose }).catch(() => {}));
  toggles.append(eye, verbose, voice);
  box.append(canvas, now, toggles);
  return box;
}

function paintEye() {
  const on = Boolean(practice?.watching);
  ui.eye.classList.toggle("is-off", !on);
  ui.eye.textContent = on ? "Ноа видит терминал" : "Ноа не смотрит";
}

/** Курсы для выбора темы — грузятся один раз, при первом показе выбора. */
let courses = null;
/** Выбранные курс и тема — до того, как сценарий составлен. */
let picked = { course: null, topic: null };

const STATUS_MARK = { done: "✓", practice: "◐", reading: "◔", new: "" };

function setup(root) {
  const box = el("div", "setup");
  root.append(box);
  if (!courses) {
    box.append(el("p", "", "Загружаю курсы…"));
    api
      .invoke("learn_overview")
      .then((list) => (courses = Array.isArray(list) ? list : []))
      .catch(() => (courses = []))
      .then(() => render());
    return;
  }
  box.append(el("h2", "", "Практика по теме"));
  box.append(
    el(
      "p",
      "",
      "Выберите тему курса — Ноа соберёт живую задачу на её понятиях: шаги в порядке урока и общую картину. Работаете вы сами в терминале слева, Ноа смотрит и подключается, когда нужна.",
    ),
  );

  const wish = el("textarea", "answer");
  wish.rows = 2;

  if (courses.length) {
    // По умолчанию — тема, из которой открыли практику, иначе текущая в курсе.
    const course = courses.find((c) => c.id === (picked.course ?? practice?.course)) ?? courses[0];
    picked.course = course.id;
    picked.topic ??= practice?.course === course.id && practice?.topic ? practice.topic : course.current ?? course.topics[0]?.id;

    if (courses.length > 1) {
      const select = el("select", "answer setup__course");
      for (const c of courses) {
        const option = el("option", "", c.title);
        option.value = c.id;
        option.selected = c.id === course.id;
        select.append(option);
      }
      select.addEventListener("change", () => {
        picked = { course: select.value, topic: null };
        render();
      });
      box.append(select);
    } else box.append(el("div", "coach__sub", course.title));

    const list = el("div", "topics-pick");
    for (const topic of course.topics) {
      const item = el("button", "topic-pick", "");
      item.append(el("span", "topic-pick__mark", STATUS_MARK[topic.status] ?? ""), el("span", "", topic.title));
      item.setAttribute("aria-pressed", String(topic.id === picked.topic));
      if (topic.id === course.current) item.title = "Тема, на которой вы остановились";
      item.addEventListener("click", () => {
        picked.topic = topic.id;
        render();
      });
      list.append(item);
    }
    box.append(list);
    wish.placeholder = "Пожелание, если есть: «на трёх серверах», «через Docker»…";
  } else {
    box.append(el("p", "", "Курсов пока нет — опишите задачу сами. Курс можно собрать в окне «Обучение»."));
    wish.placeholder = "Например: поднять кластер Kubernetes из трёх серверов";
  }

  const go = button(waiting ? "Ноа составляет сценарий…" : "Составить сценарий", () => plan(wish.value));
  go.disabled = waiting;
  wish.addEventListener("keydown", (event) => {
    if (event.key === "Enter" && !event.shiftKey) {
      event.preventDefault();
      plan(wish.value);
    }
  });
  box.append(wish, go);
  if (error) box.append(el("p", "setup__error", error));
  box.append(
    el(
      "p",
      "",
      "Вывод терминала уходит вашей модели, поэтому Ноа смотрит экономно: молчит, пока всё идёт как надо, и подключается, если что-то упало, если спросите или нажмёте «Проверь шаг». 💬 — разбирать каждую команду. Пароли и токены она закрывает, 👁 выключает наблюдение.",
    ),
  );
}

function stepCard(scenario) {
  const step = scenario.steps[practice.step];
  const card = el("section", "coach__step");
  if (practice.done) {
    card.append(el("div", "coach__step-num", "Готово"), el("h2", "coach__step-title", scenario.title));
    card.append(el("p", "coach__step-goal", scenario.goal));
  } else if (step) {
    card.append(el("div", "coach__step-num", `Шаг ${practice.step + 1} из ${scenario.steps.length}`));
    card.append(el("h2", "coach__step-title", step.title));
    if (step.concept) {
      const concept = el("span", "coach__concept", `Из урока: ${step.concept}`);
      concept.title = "Понятие урока, которое этот шаг отрабатывает руками";
      card.append(concept);
    }
    card.append(el("p", "coach__step-goal", step.goal));
    if (step.why) card.append(el("p", "coach__step-why", step.why));
    if (step.check) {
      const check = el("p", "coach__step-check");
      check.innerHTML = `<b>Проверка:</b> ${inline(step.check)}`;
      card.append(check);
    }
  }
  const actions = el("div", "coach__actions");
  // «Проверь шаг» — Ноа смотрит в терминал и сама решает, сделан ли он.
  if (!practice.done) actions.append(button("✓ Проверь шаг", () => think(() => api.invoke("practice_check"))));
  actions.append(
    button("💡 Подсказка", () => ask("Подскажи, что делать дальше на этом шаге."), true),
    button("🗺 Общая картина", () => ask("Общая картина: что уже построено, где мы сейчас, что впереди и как части связаны?"), true),
  );
  if (practice.step > 0) {
    const back = button("←", () => api.invoke("practice_step", { delta: -1 }), true);
    back.title = "Вернуться к прошлому шагу";
    actions.append(back);
  }
  if (!practice.done) {
    const skip = button("→", () => api.invoke("practice_step", { delta: 1 }), true);
    skip.title = "Считать шаг сделанным без проверки";
    actions.append(skip);
  }
  card.append(actions);
  return card;
}

function picture(scenario) {
  const scheme = parseScheme(scenario.picture);
  if (!scheme || scheme.nodes.length < 2) return null;
  const box = el("details", "coach__picture");
  box.open = pictureOpen;
  box.addEventListener("toggle", () => (pictureOpen = box.open));
  box.append(el("summary", "", "Общая картина"));
  box.append(renderScheme(scheme, { here: practice.here, down: true }));
  return box;
}
let pictureOpen = true;

function stepsList(scenario) {
  const box = el("details", "");
  box.append(el("summary", "", `Все шаги · ${Math.min(practice.step + (practice.done ? 1 : 0), scenario.steps.length)}/${scenario.steps.length}`));
  const list = el("ol", "coach__steps");
  scenario.steps.forEach((step, i) => {
    const item = el("li", "", step.title);
    if (practice.done || i < practice.step) item.classList.add("is-done");
    else if (i === practice.step) item.classList.add("is-now");
    list.append(item);
  });
  box.append(list);
  return box;
}

const feed = el("div", "coach__feed");

function paintFeed() {
  const lines = practice?.feed ?? [];
  feed.replaceChildren(
    ...lines.slice(-40).map((line) => {
      const node = el("p", `line line--${line.who === "me" ? "me" : line.kind || "comment"}`);
      node.innerHTML = inline(line.text);
      return node;
    }),
  );
  if (waiting) feed.append(el("p", "line line--hint", "Ноа думает…"));
  feed.scrollTop = feed.scrollHeight;
}

// Поле вопроса тоже одно на всё время: недописанный вопрос не теряется,
// когда Ноа присылает новое замечание.
const askRow = el("div", "coach__ask");
const askArea = el("textarea", "answer");
askArea.rows = 1;
askArea.placeholder = "Спросить Ноа — Enter";
const mic = el("button", "icon-toggle coach__mic", "🎙");
mic.title = "Спросить голосом";
mic.addEventListener("click", () => {
  listening = !listening;
  api.invoke("practice_listen", { on: listening }).catch(() => {});
  paintMic();
});
const sendBtn = el("button", "icon-toggle", "↵");
sendBtn.title = "Отправить";
sendBtn.addEventListener("click", () => send());
askRow.append(askArea, mic, sendBtn);
askArea.addEventListener("input", () => api.invoke("voice_stop").catch(() => {}));
askArea.addEventListener("keydown", (event) => {
  if (event.key === "Enter" && !event.shiftKey) {
    event.preventDefault();
    send();
  }
});

function paintMic() {
  mic.setAttribute("aria-pressed", String(listening));
  mic.title = listening ? "Слушаю — нажмите, чтобы перестать" : "Спросить голосом";
}

function send() {
  const text = askArea.value.trim();
  if (!text) return;
  askArea.value = "";
  ask(text);
}

/** Запрос к Ноа: пока она думает, это видно в ленте и на кольце. */
async function think(request) {
  if (waiting) return;
  api.invoke("voice_stop").catch(() => {});
  waiting = true;
  paintFeed();
  paintStatus();
  try {
    await request();
  } catch (err) {
    practice?.feed.push({ who: "noa", kind: "error", text: String(err) });
  }
  waiting = false;
  paintFeed();
  paintStatus();
}

const ask = (text) => think(() => api.invoke("practice_ask", { text }));

async function plan(goal) {
  if (waiting) return;
  waiting = true;
  error = "";
  render();
  try {
    const topic = courses?.length ? { course: picked.course, topic: picked.topic } : { course: null, topic: null };
    practice = await api.invoke("practice_plan", { ...topic, goal });
    picked = { course: null, topic: null };
  } catch (err) {
    error = String(err);
  }
  waiting = false;
  render();
  term.focus();
}

function render() {
  const root = ui.coach;
  root.replaceChildren(stage());
  paintEye();
  const scenario = practice?.scenario;
  ui.title.textContent = scenario?.title ?? "Практика в терминале";
  // Середина прокручивается, лента и поле вопроса всегда внизу на виду:
  // длинная схема не должна уводить их за край окна.
  const body = el("div", "coach__body");
  root.append(body);
  if (!scenario) {
    setup(body);
    if (practice?.feed?.length) {
      root.append(feedSplit, feed);
      paintFeed();
    }
    root.append(askRow);
    paintStatus();
    return;
  }
  body.append(stepCard(scenario));
  const pic = picture(scenario);
  if (pic) body.append(pic);
  body.append(stepsList(scenario));
  root.append(feedSplit, feed, askRow);
  const foot = el("div", "coach__foot");
  const fresh = el("button", "link", "↺ Новый сценарий");
  // Сценарий не теряется от случайного клика: второй клик — подтверждение.
  fresh.addEventListener("click", () => {
    if (fresh.dataset.sure) {
      api.invoke("practice_reset").catch(() => {});
      return;
    }
    fresh.dataset.sure = "1";
    fresh.textContent = "Точно новый? Нажмите ещё раз";
    setTimeout(() => {
      delete fresh.dataset.sure;
      fresh.textContent = "↺ Новый сценарий";
    }, 4000);
  });
  foot.append(fresh);
  if (scenario.setup) {
    const need = el("span", "coach__sub", `Нужно: ${scenario.setup}`);
    need.title = scenario.setup;
    foot.append(need);
  }
  root.append(foot);
  paintFeed();
  paintStatus();
}

api?.listen("practice:state", (event) => {
  const { practice: fresh, say, spoken } = event.payload ?? {};
  if (!fresh) return;
  const sameScenario = practice?.scenario?.title === fresh.scenario?.title && practice?.step === fresh.step;
  practice = fresh;
  // Лента меняется часто — панель целиком перерисовывается только при новом
  // шаге или сценарии: иначе схема мигала бы на каждое замечание.
  if (sameScenario && practice.scenario) {
    paintFeed();
    paintEye();
    // Переключатели 👁 и 💬 — по свежему состоянию.
    ui.coach.querySelector(".coach__stage")?.replaceWith(stage());
    const here = ui.coach.querySelector(".coach__picture .scheme");
    const scheme = parseScheme(practice.scenario.picture);
    if (here && scheme) here.replaceWith(renderScheme(scheme, { here: practice.here, down: true }));
  } else render();
  if (say && !spoken && aloud()) api.invoke("voice_speak", { text: say }).catch(() => {});
});

// Кольцо — по тому, что происходит: говорит, слушает, думает, смотрит.
let speaking = false;
setInterval(async () => {
  const [busy, heard] = await Promise.all([
    api.invoke("voice_busy").catch(() => false),
    api.invoke("practice_listening").catch(() => false),
  ]);
  speaking = Boolean(busy);
  if (listening && !heard) {
    // Разговор без рук кончился сам — минута тишины или «спасибо».
    listening = false;
    paintMic();
  }
  paintStatus();
}, 400);

function paintStatus() {
  const mode = speaking ? "speaking" : waiting ? "thinking" : listening ? "listening" : "idle";
  orb.setMode(mode);
  status.textContent = {
    speaking: "Ноа говорит",
    thinking: "Ноа думает…",
    listening: "Слушаю — спрашивайте",
    idle: practice?.watching ? "Ноа смотрит терминал" : "Ноа не смотрит",
  }[mode];
  sub.textContent = practice?.scenario
    ? practice.done
      ? "Сценарий пройден"
      : `Шаг ${practice.step + 1} из ${practice.scenario.steps.length}`
    : "Сценария пока нет";
}

// Esc вне терминала — «замолчи». В терминале Esc нужен самому терминалу (vim).
document.addEventListener("keydown", (event) => {
  if (event.key === "Escape" && !ui.screen.contains(document.activeElement)) api.invoke("voice_stop").catch(() => {});
});

/* ── Размеры частей окна ───────────────────────────────────────────────── */

// Границы двигаются мышью: между терминалом и Ноа и внутри панели Ноа —
// между шагом и лентой. Размеры помнятся; двойной клик — как было.
const SIZES = "noa.practiceSizes";
const sizes = (() => {
  try {
    return JSON.parse(localStorage.getItem(SIZES) ?? "{}") ?? {};
  } catch {
    return {};
  }
})();
const saveSizes = () => {
  try {
    localStorage.setItem(SIZES, JSON.stringify(sizes));
  } catch {
    /* не запомнится — не беда */
  }
};
const clamp = (value, min, max) => Math.max(min, Math.min(max, value));

function applySizes() {
  const root = document.documentElement.style;
  if (sizes.coach) root.setProperty("--coach-width", `${clamp(sizes.coach, 300, innerWidth * 0.7)}px`);
  else root.removeProperty("--coach-width");
  if (sizes.feed) ui.coach.style.setProperty("--feed-height", `${sizes.feed}px`);
  else ui.coach.style.removeProperty("--feed-height");
}

/** Тянуть границу: `onMove` получает событие указателя. */
function draggable(handle, onMove, onReset) {
  handle.addEventListener("pointerdown", (event) => {
    if (event.button !== 0) return;
    event.preventDefault();
    handle.setPointerCapture(event.pointerId);
    handle.classList.add("is-dragging");
    const move = (ev) => {
      onMove(ev);
      applySizes();
    };
    const up = () => {
      handle.classList.remove("is-dragging");
      handle.removeEventListener("pointermove", move);
      saveSizes();
    };
    handle.addEventListener("pointermove", move);
    handle.addEventListener("pointerup", up, { once: true });
    handle.addEventListener("pointercancel", up, { once: true });
  });
  handle.addEventListener("dblclick", () => {
    onReset();
    applySizes();
    saveSizes();
  });
}

draggable(
  ui.split,
  (event) => (sizes.coach = clamp(innerWidth - event.clientX, 300, innerWidth * 0.7)),
  () => delete sizes.coach,
);

// Граница ленты живёт в панели, которая перерисовывается, — создаётся один раз.
const feedSplit = el("div", "coach__split");
feedSplit.setAttribute("role", "separator");
feedSplit.title = "Потяните, чтобы поменять высоту ленты · двойной клик — как было";
draggable(
  feedSplit,
  (event) => {
    const box = ui.coach.getBoundingClientRect();
    const below = askRow.offsetHeight + (ui.coach.querySelector(".coach__foot")?.offsetHeight ?? 0) + 24;
    sizes.feed = clamp(box.bottom - event.clientY - below, 90, box.height * 0.75);
  },
  () => delete sizes.feed,
);
addEventListener("resize", applySizes);
applySizes();

/* ── Начало ────────────────────────────────────────────────────────────── */

(async () => {
  await document.fonts?.ready;
  term.open(ui.screen);
  // Размер — после того как окно разложилось: замер раньше давал две
  // колонки, и первое приглашение оболочки ломалось по два знака в строке.
  const frame = () => new Promise((resolve) => requestAnimationFrame(resolve));
  for (let tries = 0; tries < 20; tries++) {
    await frame();
    fit.fit();
    if (term.cols >= 40) break;
  }
  await startShell();
  term.focus();
  practice = await api.invoke("practice_state").catch(() => null);
  render();
})();
