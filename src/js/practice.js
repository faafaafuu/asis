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
  toggles.append(eye, voice);
  box.append(canvas, now, toggles);
  return box;
}

function paintEye() {
  const on = Boolean(practice?.watching);
  ui.eye.classList.toggle("is-off", !on);
  ui.eye.textContent = on ? "Ноа видит терминал" : "Ноа не смотрит";
}

const EXAMPLES = [
  "Поднять k3s-кластер из трёх узлов и запустить в нём приложение",
  "Nginx перед приложением в Docker, с HTTPS от Let's Encrypt",
  "PostgreSQL с репликой и резервной копией",
  "Свой VPN на WireGuard",
  "Мониторинг: Prometheus и Grafana",
];

function setup(root) {
  const box = el("div", "setup");
  box.append(el("h2", "", "Что строим?"));
  box.append(
    el(
      "p",
      "",
      "Опишите задачу — Ноа составит сценарий по шагам и общую картину. Работаете вы сами в терминале слева: подключитесь к своему серверу, а Ноа смотрит, объясняет, что происходит, и подсказывает, куда дальше.",
    ),
  );
  const area = el("textarea", "answer");
  area.placeholder = "Например: поднять кластер Kubernetes из трёх серверов";
  const chips = el("div", "chips");
  if (practice?.topic) {
    const chip = el("button", "chip chip--topic", "По теме, которую сейчас прохожу");
    chip.addEventListener("click", () => plan(""));
    chips.append(chip);
  }
  for (const text of EXAMPLES) {
    const chip = el("button", "chip", text);
    chip.addEventListener("click", () => {
      area.value = text;
      area.focus();
    });
    chips.append(chip);
  }
  const go = button(waiting ? "Ноа составляет сценарий…" : "Составить сценарий", () => plan(area.value));
  go.disabled = waiting;
  area.addEventListener("keydown", (event) => {
    if (event.key === "Enter" && !event.shiftKey) {
      event.preventDefault();
      plan(area.value);
    }
  });
  box.append(area, chips, go);
  if (error) box.append(el("p", "setup__error", error));
  box.append(
    el(
      "p",
      "",
      "Можно и без сценария: просто работайте — Ноа будет объяснять, что вы делаете. Вывод терминала уходит вашей модели; пароли и токены Ноа закрывает, а глаз 👁 выключает наблюдение совсем.",
    ),
  );
  root.append(box);
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
    card.append(el("p", "coach__step-goal", step.goal));
    if (step.why) card.append(el("p", "coach__step-why", step.why));
    if (step.check) {
      const check = el("p", "coach__step-check");
      check.innerHTML = `<b>Проверка:</b> ${inline(step.check)}`;
      card.append(check);
    }
  }
  const actions = el("div", "coach__actions");
  if (!practice.done) actions.append(button("✓ Шаг готов", () => api.invoke("practice_step", { delta: 1 })));
  actions.append(
    button("💡 Подсказка", () => ask("Подскажи, что делать дальше на этом шаге."), true),
    button("🗺 Общая картина", () => ask("Общая картина: что уже построено, где мы сейчас, что впереди и как части связаны?"), true),
  );
  if (practice.step > 0) {
    const back = button("←", () => api.invoke("practice_step", { delta: -1 }), true);
    back.title = "Вернуться к прошлому шагу";
    actions.append(back);
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

async function ask(text) {
  api.invoke("voice_stop").catch(() => {});
  waiting = true;
  paintFeed();
  try {
    await api.invoke("practice_ask", { text });
  } catch (err) {
    practice?.feed.push({ who: "noa", kind: "error", text: String(err) });
  }
  waiting = false;
  paintFeed();
}

async function plan(goal) {
  if (waiting) return;
  waiting = true;
  error = "";
  render();
  try {
    practice = await api.invoke("practice_plan", { goal });
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
      root.append(feed);
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
  root.append(feed, askRow);
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
    const eye = ui.coach.querySelector(".coach__toggles .icon-toggle");
    eye?.setAttribute("aria-pressed", String(Boolean(practice.watching)));
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
