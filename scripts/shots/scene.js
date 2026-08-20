// Кадры для README: настоящий попап и настоящий индикатор, снятые покадрово.
//
// Почему так, а не запись экрана. Запись пришлось бы делать вручную и заново
// после каждой правки вёрстки, а половина того, что нужно показать, начинается
// с голоса — его в записи не видно. Здесь же кадр задаётся номером: страница
// собирает ровно то состояние, которое ему соответствует, и headless-браузер
// его снимает. Разметка, стили и шрифты — те же самые файлы, что в программе.

import { PopupView } from "../../src/js/popup-view.js";

const params = new URLSearchParams(location.search);
const scene = params.get("scene") ?? "select";
const frame = Number(params.get("f") ?? 0);

/* ── Сцена «выделил слово» ────────────────────────────────────────────────── */

const TEXT_BEFORE =
  "Снимок сделан в ближнем инфракрасном диапазоне, поэтому снег и облака на нём почти неразличимы: ";
const TERM = "альбедо";
const TEXT_AFTER =
  " у них близко, и обе поверхности отражают почти всё, что на них падает. Разделить их удаётся только по данным теплового канала, где разница выходит куда заметнее.";

const DEFINITION =
  "Доля света, которую поверхность отражает обратно, а не поглощает. Свежий снег отражает почти всё, вспаханная земля — почти ничего.";
const SIMPLE = "Насколько поверхность «белая» для света: чем выше альбедо, тем больше она отражает.";
const EXAMPLES = [
  "Свежий снег — около 0,9: отражает девять десятых света.",
  "Асфальт — около 0,1, поэтому летом он так раскаляется.",
];

/** Плавная кривая: быстрый старт, мягкий приход. */
const ease = (t) => 1 - Math.pow(1 - Math.min(Math.max(t, 0), 1), 3);

/** Сколько первых символов строки показать на этом кадре. */
const typed = (text, from, frames) =>
  text.slice(0, Math.round(text.length * ease((frame - from) / frames)));

function buildPage(highlight) {
  const page = document.getElementById("page");
  page.innerHTML = "";
  page.append(document.createTextNode(TEXT_BEFORE));
  const mark = document.createElement("span");
  mark.className = "sel";
  mark.textContent = TERM;
  // Выделение наползает на слово слева направо — так же, как его тянут мышью.
  mark.style.background = `linear-gradient(90deg,
      rgba(122,162,247,0.34) ${highlight * 100}%,
      rgba(122,162,247,0) ${highlight * 100}%)`;
  if (highlight > 0.99) mark.style.background = "rgba(122,162,247,0.34)";
  page.append(mark);
  page.append(document.createTextNode(TEXT_AFTER));
  return mark;
}

function selectScene() {
  const highlight = ease((frame - 6) / 5);
  const mark = buildPage(highlight);

  if (frame < 12) return;

  const view = new PopupView({
    client: { explain: () => new Promise(() => {}), ask: () => new Promise(() => {}) },
  });
  view.dialogue = true;
  const stage = document.getElementById("stage");
  stage.append(view.el);

  // Попап встаёт под выделенным словом — там же, где его ставит программа.
  const box = mark.getBoundingClientRect();
  stage.style.left = `${Math.round(box.left - 18)}px`;
  stage.style.top = `${Math.round(box.bottom + 12)}px`;

  const appear = ease((frame - 12) / 3);
  stage.style.opacity = String(appear);
  stage.style.transform = `translateY(${(1 - appear) * 6}px) scale(${0.985 + appear * 0.015})`;

  const state = view.state;
  state.term = TERM;
  state.context = TEXT_BEFORE;

  if (frame < 18) {
    state.phase = "loading";
  } else {
    state.phase = "success";
    state.data = { def: typed(DEFINITION, 18, 20), simple: "", examples: [] };
  }

  if (frame >= 48) {
    state.expanded = true;
    state.data = {
      def: DEFINITION,
      simple: typed(SIMPLE, 48, 8),
      examples: frame >= 58 ? EXAMPLES.slice(0, frame >= 63 ? 2 : 1) : [],
    };
  }

  view.render();
}

/* ── Сцена «позвал по имени» ──────────────────────────────────────────────── */

const SPOKEN = "Ноа, что такое альбедо?";
const ANSWER =
  "Доля света, которую поверхность отражает обратно. У свежего снега почти единица, у асфальта около одной десятой.";

/** Состояние индикатора на этом кадре — как его меняет программа по ходу разговора. */
function hudMode() {
  if (frame < 26) return "listening";
  if (frame < 40) return "thinking";
  return "speaking";
}

function voiceScene() {
  // Индикатор берём настоящий: он ждёт события из Rust, и мы их ему даём.
  globalThis.__TAURI__ = {
    core: { invoke: async () => hudMode() },
    event: {
      listen: (event, handler) => {
        if (event === "hud:mode") handler({ payload: hudMode() });
        return Promise.resolve(() => {});
      },
    },
  };

  const wrap = document.getElementById("hudwrap");
  wrap.style.opacity = String(ease((frame - 2) / 4));

  const caption = document.getElementById("caption");
  if (frame >= 6) {
    caption.style.opacity = String(ease((frame - 6) / 4));
    caption.innerHTML = `<i>«</i>${typed(SPOKEN, 6, 14)}<i>»</i>`;
  }

  if (frame >= 34) {
    const view = new PopupView({
      client: { explain: () => new Promise(() => {}), ask: () => new Promise(() => {}) },
    });
    view.dialogue = true;
    const stage = document.getElementById("stage");
    stage.append(view.el);
    stage.style.left = "50%";
    stage.style.top = "212px";
    stage.style.transformOrigin = "50% 0";

    const appear = ease((frame - 34) / 4);
    stage.style.opacity = String(appear);
    stage.style.transform = `translateX(-50%) translateY(${(1 - appear) * 8}px) scale(${
      0.985 + appear * 0.015
    })`;

    const state = view.state;
    state.term = "";
    state.phase = frame < 40 ? "loading" : "success";
    state.data = { def: typed(ANSWER, 40, 22), simple: "", examples: [] };
    view.render();
  }

  return import("../../src/js/hud.js");
}

/* ── Запуск ───────────────────────────────────────────────────────────────── */

if (scene === "voice") {
  await voiceScene();
} else {
  selectScene();
}
