// Ядро приложения «NOAH Учёба»: связь с программой, навигация, общие куски
// экранов. Экраны — функции, которые рисуют себя в узел и сами грузят
// данные; навигация — стек экранов у каждой из четырёх вкладок.

import { tauri, isWebNoa } from "../bridge.js";

export const api = tauri();
/** Открыто в браузере (noahlab.ru/learn): команды — Ноа онлайн, терминала нет. */
export const isWeb = isWebNoa();

/* ── Элементы ──────────────────────────────────────────────────────────── */

export function el(tag, className = "", text = "") {
  const node = document.createElement(tag);
  if (className) node.className = className;
  if (text !== "" && text != null) node.textContent = String(text);
  if (tag === "button") node.type = "button";
  return node;
}

/** Иконка из спрайта: `<svg class="icon"><use href="#i-…"/></svg>`. */
export function icon(name, size = 24, stroke = 1.75) {
  const svg = document.createElementNS("http://www.w3.org/2000/svg", "svg");
  svg.setAttribute("class", "icon");
  svg.setAttribute("width", size);
  svg.setAttribute("height", size);
  svg.style.width = `${size}px`;
  svg.style.height = `${size}px`;
  svg.style.strokeWidth = String(stroke);
  const use = document.createElementNS("http://www.w3.org/2000/svg", "use");
  use.setAttribute("href", `#i-${name}`);
  svg.append(use);
  return svg;
}

export function button(text, onClick, className = "btn btn--secondary") {
  const node = el("button", className);
  if (text) node.append(text);
  node.addEventListener("click", onClick);
  return node;
}

export function iconButton(name, title, onClick, size = 24) {
  const node = el("button", "icon-btn");
  node.title = title;
  node.setAttribute("aria-label", title);
  node.append(icon(name, size, size < 24 ? 2 : 1.75));
  node.addEventListener("click", onClick);
  return node;
}

export function label(text) {
  return el("span", "label", text);
}

export function progress(percent, className = "bar3") {
  const bar = el("div", className);
  const fill = el("span");
  fill.style.width = `${Math.max(0, Math.min(100, Math.round(percent)))}%`;
  bar.append(fill);
  return bar;
}

/** Сегменты шагов: `done` готовых, текущий — `now`. */
export function steps(total, done, now = -1, { thin = false, ok = false } = {}) {
  const row = el("div", thin ? "steps steps--thin" : "steps");
  row.style.gridTemplateColumns = `repeat(${Math.max(total, 1)}, minmax(0, 1fr))`;
  for (let i = 0; i < total; i++) {
    const seg = el("span");
    if (i === now) seg.className = "is-now";
    else if (i < done) seg.className = ok ? "is-done" : "is-on";
    row.append(seg);
  }
  return row;
}

export function loading(text = "Загружаю…") {
  const box = el("div", "loading");
  box.append(el("span", "spinner"), el("span", "", text));
  return box;
}

export const plural = (n, one, few, many) => {
  const mod10 = n % 10;
  const mod100 = n % 100;
  if (mod10 === 1 && mod100 !== 11) return `${n} ${one}`;
  if (mod10 >= 2 && mod10 <= 4 && (mod100 < 12 || mod100 > 14)) return `${n} ${few}`;
  return `${n} ${many}`;
};

/* ── Хранилище настроек ────────────────────────────────────────────────── */

export const prefs = {
  get(key, fallback = null) {
    try {
      const value = localStorage.getItem(`learn.${key}`);
      return value === null ? fallback : JSON.parse(value);
    } catch {
      return fallback;
    }
  },
  set(key, value) {
    try {
      localStorage.setItem(`learn.${key}`, JSON.stringify(value));
    } catch {
      /* не запомнится — не беда */
    }
  },
};

/* ── Связь ─────────────────────────────────────────────────────────────── */

let banner = null;

function showNet(kind, text, onNow) {
  hideNet();
  banner = el("div", kind === "off" ? "net is-off" : "net");
  banner.append(icon(kind === "off" ? "offline" : "slow", 18, 2), el("span", "net__text", text));
  if (onNow) banner.append(button("Сейчас", onNow, "btn"));
  document.body.append(banner);
}

function hideNet() {
  banner?.remove();
  banner = null;
}

const NETWORK = /сет|связ|недоступн|timeout|таймаут|не успел|не ответил|connection|network|error sending|отвечает|502|503|504/i;

/**
 * Вызов программы. `slow` — запрос к модели или сайту: через 15 с видно
 * «Медленно…», сбой связи — «Нет связи · повтор через N с» и сам повтор.
 * Прочие ошибки — сразу наверх, вызывающему.
 */
export async function call(cmd, args = {}, { slow = false, retries = 4 } = {}) {
  if (!api) throw new Error("Нет связи с программой.");
  if (!slow) return api.invoke(cmd, args);
  for (let attempt = 0; ; attempt++) {
    // Ответ модели через мост — секунд десять: плашка — только когда дольше.
    const timer = setTimeout(() => showNet("slow", "Медленно… Ноа ждёт ответа"), 15000);
    try {
      const result = await api.invoke(cmd, args);
      clearTimeout(timer);
      hideNet();
      return result;
    } catch (err) {
      clearTimeout(timer);
      hideNet();
      if (attempt >= retries || !NETWORK.test(String(err))) throw err;
      const wait = [5, 10, 20, 40][Math.min(attempt, 3)];
      await new Promise((resolve) => {
        let left = wait;
        let done = false;
        const finish = () => {
          if (done) return;
          done = true;
          clearInterval(tick);
          hideNet();
          resolve();
        };
        const draw = () => showNet("off", `Нет связи · повтор через ${left} с`, finish);
        draw();
        const tick = setInterval(() => {
          left -= 1;
          if (left <= 0) finish();
          else draw();
        }, 1000);
      });
    }
  }
}

export function toast(text, ms = 2600) {
  const node = el("div", "toast", text);
  document.body.append(node);
  setTimeout(() => node.remove(), ms);
}

/* ── Голос ─────────────────────────────────────────────────────────────── */

export const voice = {
  aloud() {
    return prefs.get("aloud", true);
  },
  /** `pace` — темп относительно настройки голоса: 0.9 — чуть медленнее. */
  async speak(text, { pace } = {}) {
    if (!text?.trim()) return;
    try {
      await api.invoke("voice_speak", { text, ...(pace ? { pace } : {}) });
    } catch {
      /* голос выключен — молчим */
    }
  },
  stop() {
    api?.invoke("voice_stop").catch(() => {});
  },
  busy() {
    return api ? api.invoke("voice_busy").catch(() => false) : Promise.resolve(false);
  },
  /**
   * Слушает одну фразу (системное распознавание телефона). `pause` — сколько
   * секунд тишины считать концом фразы: разговору — короче, пересказу — дольше.
   * Для долгих ответов: `idle` — сколько ждать, пока заговорят, `limit` —
   * сколько слушать всего, `ends` — концовки («вот как-то так»): сказал —
   * фраза отдаётся сразу; `asks` — начала коротких встречных вопросов.
   */
  async listen({ pause, ...more } = {}) {
    const heard = await api.invoke("plugin:sufler|listen", { lang: "ru-RU", ...(pause ? { pause } : {}), ...more });
    return String(heard?.text ?? "").trim();
  },
  cancel() {
    api?.invoke("plugin:sufler|cancelListening").catch(() => {});
  },
};

/* ── Отклик ────────────────────────────────────────────────────────────── */

/**
 * Отклик вибромотором: `light` — касание, `select` — переключение,
 * `medium` — меню по удержанию, `warning` / `success` — удаление, готово.
 * На iPhone — Taptic Engine через плагин; в браузере — где он умеет.
 */
export function haptic(style = "light") {
  if (!isWeb) {
    api?.invoke("plugin:sufler|haptic", { style })?.catch?.(() => {});
    return;
  }
  try {
    navigator.vibrate?.(style === "light" || style === "select" ? 8 : 18);
  } catch {
    /* без отклика */
  }
}

/** Движение выключено в настройках телефона — переходы без анимации. */
export const still = () => Boolean(window.matchMedia?.("(prefers-reduced-motion: reduce)").matches);

/** Кривая и длительность переходов — как у системных экранов iPhone. */
export const EASE = "cubic-bezier(0.2, 0.85, 0.25, 1)";
const SLIDE = 340;
/** Насколько уезжает нижний экран, пока сверху въезжает новый. */
const PARALLAX = 28;

/* ── Навигация ─────────────────────────────────────────────────────────── */

const TABS = [
  { id: "courses", label: "Курсы", icon: "learn", root: "courses" },
  { id: "review", label: "Повторение", icon: "cards", root: "review" },
  { id: "focus", label: "Фокус", icon: "timer", root: "focus" },
  { id: "profile", label: "Профиль", icon: "user", root: "profile" },
];

const screens = {};
// Запись стека: экран и его параметры; `snap` — как выглядел экран, когда с
// него ушли дальше (живой узел без обработчиков): он лежит под верхним при
// свайпе назад и закрывает свежую отрисовку, пока та грузится.
const stacks = Object.fromEntries(TABS.map((tab) => [tab.id, [{ name: tab.root, params: {} }]]));
let current = "courses";
let host = null;
/** Сцена: тут живут экраны, в переходе — два сразу. Вкладки — под ней. */
let stage = null;
let tabs = null;
let screenEl = null;
let teardown = null;
let badge = 0;

export function register(name, render) {
  screens[name] = render;
}

/**
 * В браузере переходы — записи истории: «назад» браузера, свайп Safari от
 * края и кнопка «назад» Android ведут по экранам, а не уводят с сайта.
 */
const pages = {
  depth: 0,
  ignore: 0,
  push() {
    if (!isWeb) return;
    this.depth += 1;
    window.history.pushState({ learn: this.depth }, "");
  },
  back() {
    if (!isWeb || !this.depth) return;
    this.depth -= 1;
    this.ignore += 1;
    window.history.back();
  },
  reset() {
    if (!isWeb || !this.depth) return;
    this.ignore += 1;
    window.history.go(-this.depth);
    this.depth = 0;
  },
};
if (isWeb) {
  window.addEventListener("popstate", () => {
    if (pages.ignore) {
      pages.ignore -= 1;
      return;
    }
    pages.depth = Math.max(0, pages.depth - 1);
    // Браузер уже показал свой переход — без второго.
    if (stacks[current].length > 1) {
      stacks[current].pop();
      draw("none");
    }
  });
}

export const nav = {
  push(name, params = {}) {
    stacks[current].push({ name, params });
    pages.push();
    draw("push");
  },
  replace(name, params = {}) {
    stacks[current][stacks[current].length - 1] = { name, params };
    draw("fade");
  },
  back() {
    if (stacks[current].length < 2) return draw("none");
    stacks[current].pop();
    pages.back();
    draw("pop");
  },
  /** К корню вкладки — например, после конца экзамена. */
  home() {
    const deeper = stacks[current].length > 1;
    stacks[current].length = 1;
    pages.reset();
    draw(deeper ? "pop" : "none");
  },
  tab(id, reset = false) {
    const same = id === current;
    const deeper = stacks[id].length > 1;
    current = id;
    if (reset) stacks[id].length = 1;
    pages.reset();
    if (!same) haptic("select");
    draw(same ? (reset && deeper ? "pop" : "none") : "fade");
  },
  refresh() {
    draw("none");
  },
  setBadge(n) {
    badge = n;
    const count = tabs?.querySelector(".tab[data-tab='review'] .tab__count");
    if (count) {
      count.textContent = String(n);
      count.hidden = !n;
    }
  },
  get current() {
    return current;
  },
  /** Сколько экранов в стеке вкладки: 1 — её главный экран. */
  get depth() {
    return stacks[current].length;
  },
};

function tabBar() {
  const bar = el("nav", "tabs");
  const row = el("div", "tabs__row");
  for (const tab of TABS) {
    const item = el("button", "tab");
    item.dataset.tab = tab.id;
    item.append(icon(tab.icon), tab.label);
    if (tab.id === "review") {
      const count = el("span", "tab__count", badge);
      count.hidden = !badge;
      item.append(count);
    }
    // Повторный тап по своей вкладке — к её корню.
    item.addEventListener("click", () => nav.tab(tab.id, tab.id === current));
    row.append(item);
  }
  bar.append(row);
  return bar;
}

const scrollOf = (screen) => screen?.querySelector(".content")?.scrollTop ?? 0;
const scrollTo = (screen, top) => {
  const box = screen?.querySelector(".content");
  if (box && top) box.scrollTop = top;
};

/** Экран вне потока: уходящий, подложка или крышка поверх свежего. */
function lift(screen, z) {
  screen.classList.add("screen--off");
  screen.style.zIndex = String(z);
}

/** Анимация, после которой стиль остаётся конечным, а сама она снимается. */
function play(node, frames, duration, easing = EASE) {
  const run = node.animate(frames, { duration, easing, fill: "forwards" });
  return run.finished.then(
    () => {
      const last = frames[frames.length - 1];
      for (const [key, value] of Object.entries(last)) if (key !== "offset") node.style[key] = value;
      run.cancel();
    },
    () => {},
  );
}

/**
 * Свежий экран дорисован — данные пришли (нет «Загружаю…») или прошло
 * полсекунды: тогда снимается крышка и возвращается прокрутка.
 */
function whenReady(screen, done) {
  const from = performance.now();
  const tick = () => {
    if (!screen.isConnected || !screen.querySelector(".loading") || performance.now() - from > 450) return done();
    requestAnimationFrame(tick);
  };
  requestAnimationFrame(tick);
}

function uncover(screen, cover, scroll) {
  if (!cover) return;
  whenReady(screen, () => {
    scrollTo(screen, scroll);
    if (!cover.isConnected) return;
    if (still()) return cover.remove();
    play(cover, [{ opacity: getComputedStyle(cover).opacity }, { opacity: 0 }], 140, "ease-out").then(() => {
      cover.remove();
      cover.style.opacity = "";
    });
  });
}

/**
 * Рисует верхний экран вкладки. `motion` — как он появляется: `push` —
 * въезжает справа поверх прежнего, `pop` — прежний уезжает вправо и
 * открывает этот, `swiped` — то же, но экран уже утащен пальцем, `fade` —
 * смена вкладки, `none` — перерисовка на месте (без мигания «Загружаю…»).
 */
function draw(motion = "none") {
  const old = screenEl;
  const scroll = scrollOf(old);
  // Недоигравший переход — сразу в конец.
  for (const node of [...stage.children]) if (node !== old) node.remove();
  for (const run of old?.getAnimations?.() ?? []) run.cancel();
  try {
    teardown?.();
  } catch {
    /* экран ушёл с ошибкой — всё равно рисуем следующий */
  }
  teardown = null;
  voice.stop();

  const stack = stacks[current];
  const top = stack[stack.length - 1];
  if (motion === "push" && old && stack.length > 1) Object.assign(stack[stack.length - 2], { snap: old, scroll });
  let cover = null;
  let restore = 0;
  if ((motion === "pop" || motion === "swiped") && top.snap) {
    cover = top.snap;
    restore = top.scroll ?? 0;
  } else if (motion === "none" && old) {
    cover = old;
    restore = scroll;
  }
  top.snap = null;

  if (old && old !== cover) lift(old, motion === "push" ? 1 : 3);
  if (cover) {
    lift(cover, 2);
    cover.style.transform = "";
    cover.style.opacity = "";
    if (!cover.isConnected) {
      stage.append(cover);
      scrollTo(cover, restore);
    }
  }

  const render = screens[top.name];
  const screen = el("section", "screen");
  screenEl = screen;
  stage.prepend(screen);
  let full = false;
  try {
    const view = render(screen, top.params) ?? {};
    full = Boolean(view.full);
    teardown = view.cleanup ?? null;
  } catch (err) {
    screen.append(header({ title: "Ошибка", back: stack.length > 1 }), el("div", "content", String(err)));
  }
  tabs.hidden = full;
  for (const item of tabs.querySelectorAll(".tab")) {
    if (item.dataset.tab === current) item.setAttribute("aria-current", "page");
    else item.removeAttribute("aria-current");
  }
  // Кольцо Ноа — только на экранах с вкладками: в уроке и практике свой микрофон.
  document.body.classList.toggle("is-full", full);

  const drop = () => old?.remove();
  if (!old || still() || motion === "swiped") {
    drop();
    if (cover && cover !== old) uncover(screen, cover, restore);
    else cover?.remove();
    return;
  }
  if (motion === "none") return uncover(screen, cover, restore);
  if (motion === "fade") {
    drop();
    play(screen, [{ opacity: 0, transform: "translateY(6px)" }, { opacity: 1, transform: "none" }], 200, "ease-out");
    return;
  }
  if (motion === "push") {
    screen.style.zIndex = "2";
    screen.classList.add("screen--front");
    play(old, [{ transform: "none", opacity: 1 }, { transform: `translateX(-${PARALLAX}%)`, opacity: 0.35 }], SLIDE).then(drop);
    play(screen, [{ transform: "translateX(100%)" }, { transform: "none" }], SLIDE).then(() => {
      screen.classList.remove("screen--front");
      screen.style.zIndex = "";
    });
    return;
  }
  // pop: прежний уезжает вправо, под ним — подложка, она же крышка.
  old.classList.add("screen--front");
  play(old, [{ transform: "none" }, { transform: "translateX(100%)" }], SLIDE - 40).then(drop);
  const under = cover ?? screen;
  play(under, [{ transform: `translateX(-${PARALLAX}%)`, opacity: 0.35 }, { transform: "none", opacity: 1 }], SLIDE - 40).then(() => {
    under.style.transform = "";
    under.style.opacity = "";
    if (cover) uncover(screen, cover, restore);
  });
}

/* ── Свайп от левого края — назад, как в приложениях iPhone ──────────────
   Экран идёт за пальцем, под ним — прежний (его снимок). Отпустили дальше
   трети ширины или смахнули быстро — назад; иначе экран возвращается. В
   браузере этот жест — у самого Safari (записи истории выше). */

const EDGE = 30;

function swipeBack() {
  let drag = null;
  const backButton = () =>
    screenEl?.querySelector(':scope > header [aria-label="Назад"], :scope > header [aria-label="Закрыть"]') ?? null;

  stage.addEventListener(
    "touchstart",
    (event) => {
      drag = null;
      if (event.touches.length !== 1 || stacks[current].length < 2) return;
      if (document.querySelector(".sheet, .ctx, .act")) return;
      const touch = event.touches[0];
      const box = stage.getBoundingClientRect();
      if (touch.clientX - box.left > EDGE) return;
      const back = backButton();
      if (!back) return;
      drag = { x: touch.clientX, y: touch.clientY, on: false, width: box.width, back, dx: 0, trail: [] };
    },
    { passive: true },
  );

  stage.addEventListener(
    "touchmove",
    (event) => {
      if (!drag) return;
      const touch = event.touches[0];
      const dx = touch.clientX - drag.x;
      const dy = touch.clientY - drag.y;
      if (!drag.on) {
        if (Math.abs(dy) > 12 && Math.abs(dy) > Math.abs(dx)) {
          drag = null;
          return;
        }
        if (dx < 8) return;
        begin(drag);
      }
      event.preventDefault();
      drag.dx = Math.max(0, dx);
      drag.trail.push([event.timeStamp, touch.clientX]);
      if (drag.trail.length > 6) drag.trail.shift();
      follow(drag);
    },
    { passive: false },
  );

  const end = (cancelled) => {
    const done = drag;
    drag = null;
    if (done?.on) release(done, cancelled);
  };
  stage.addEventListener("touchend", () => end(false));
  stage.addEventListener("touchcancel", () => end(true));

  function begin(state) {
    state.on = true;
    state.screen = screenEl;
    for (const run of screenEl.getAnimations()) run.cancel();
    const below = stacks[current][stacks[current].length - 2];
    state.under = below?.snap ?? null;
    if (state.under) {
      lift(state.under, 1);
      stage.append(state.under);
      scrollTo(state.under, below.scroll);
    }
    screenEl.style.zIndex = "2";
    screenEl.classList.add("screen--front");
    // Поле ввода под пальцем — клавиатура убирается сразу.
    document.activeElement?.blur?.();
  }

  function follow(state) {
    const k = state.dx / state.width;
    state.screen.style.transform = `translateX(${state.dx}px)`;
    if (state.under) {
      state.under.style.transform = `translateX(${-PARALLAX * (1 - k)}%)`;
      state.under.style.opacity = String(0.35 + 0.65 * k);
    }
  }

  function release(state, cancelled) {
    const first = state.trail[0];
    const last = state.trail[state.trail.length - 1];
    const speed = first && last && last[0] > first[0] ? (last[1] - first[1]) / (last[0] - first[0]) : 0;
    const go = !cancelled && (state.dx > state.width * 0.35 || (speed > 0.45 && state.dx > 24));
    const left = go ? 1 - state.dx / state.width : state.dx / state.width;
    const time = Math.max(140, Math.round(280 * left));
    const custom = state.back.dataset.back === "custom";
    const screen = state.screen;
    const under = state.under;
    if (go && !custom) {
      play(screen, [{ transform: screen.style.transform }, { transform: "translateX(100%)" }], time);
      const finish = () => {
        if (screenEl !== screen) return;
        stacks[current].pop();
        pages.back();
        draw("swiped");
      };
      if (under) play(under, [{ transform: under.style.transform, opacity: under.style.opacity }, { transform: "none", opacity: 1 }], time).then(finish);
      else setTimeout(finish, time);
      return;
    }
    // Не дотянули (или у экрана свой «назад» — он спросит сам): на место.
    play(screen, [{ transform: screen.style.transform }, { transform: "none" }], time).then(() => {
      screen.style.transform = "";
      screen.style.zIndex = "";
      screen.classList.remove("screen--front");
      if (go && custom) state.back.click();
    });
    if (under)
      play(under, [{ transform: under.style.transform, opacity: under.style.opacity }, { transform: `translateX(-${PARALLAX}%)`, opacity: 0.35 }], time).then(() => {
        under.remove();
        under.style.transform = "";
        under.style.opacity = "";
      });
  }
}

export function start(root, first = "courses") {
  host = root;
  current = first;
  stage = el("div", "stage");
  tabs = tabBar();
  host.replaceChildren(stage, tabs);
  // :active на iPhone срабатывает, только если странице нужны касания.
  document.addEventListener("touchstart", () => {}, { passive: true });
  if (!isWeb) swipeBack();
  draw();
}

/* ── Шапки ─────────────────────────────────────────────────────────────── */

/**
 * Шапка экрана. `home` — главная раздела (знак, NOAH, УЧЁБА, микрофон);
 * `big` — заголовок раздела крупно; иначе — «Назад», заголовок, «Ещё».
 */
export function header({ home = false, big = "", meta = null, title = "", back = true, close = false, onBack = null, right = null, onMic = null } = {}) {
  const bar = el("header", home ? "bar bar--home" : big ? "bar bar--title" : "bar");
  if (home) {
    const mark = el("span", "bar__mark");
    const img = el("img");
    img.src = "assets/noah-mark.svg";
    img.alt = "";
    mark.append(img);
    bar.append(mark, el("span", "bar__name", "NOAH"), el("span", "bar__app", "Учёба"), el("span", "grow"));
    if (onMic) bar.append(iconButton("mic", "Спросить Ноа", onMic));
    return bar;
  }
  if (big) {
    bar.append(el("span", "bar__big", big));
    if (meta) {
      const box = el("span", "bar__meta");
      box.append(meta);
      bar.append(box);
    }
    return bar;
  }
  if (back || close) {
    const btn = iconButton(close ? "close" : "arrow-left", close ? "Закрыть" : "Назад", onBack ?? (() => nav.back()));
    // Свайп от края делает то же; свой «назад» (спросить, выйти ли) — жмётся.
    btn.dataset.back = onBack ? "custom" : "nav";
    bar.append(btn);
  }
  bar.append(el("span", "bar__title", title));
  if (right) bar.append(right);
  return bar;
}

/* ── Лист поверх экрана ────────────────────────────────────────────────── */

/**
 * Лист снизу: выезжает, закрывается крестиком, тапом мимо или смахиванием
 * вниз — за ручку, шапку или содержимое, прокрученное к началу.
 */
export function sheet(title, fill) {
  const layer = el("div", "sheet");
  const body = el("div", "sheet__body");
  const head = el("div", "sheet__head");
  let closed = false;
  const close = () => {
    if (closed) return;
    closed = true;
    voice.stop();
    layer.style.pointerEvents = "none";
    if (still()) return layer.remove();
    const from = getComputedStyle(body).transform;
    play(body, [{ transform: from === "none" ? "translateY(0)" : from }, { transform: "translateY(105%)" }], 220, "cubic-bezier(0.4, 0, 1, 1)");
    play(layer, [{ backgroundColor: getComputedStyle(layer).backgroundColor }, { backgroundColor: "rgba(2, 4, 7, 0)" }], 220, "ease-in").then(() => layer.remove());
  };
  head.append(el("span", "sheet__title", title), iconButton("close", "Закрыть", close));
  const content = el("div", "sheet__content");
  body.append(el("span", "sheet__grab"), head, content);
  layer.append(body);
  layer.addEventListener("click", (event) => {
    if (event.target === layer) close();
  });
  document.body.append(layer);
  if (!still()) {
    play(body, [{ transform: "translateY(100%)" }, { transform: "translateY(0)" }], 320).then(() => (body.style.transform = ""));
    play(layer, [{ backgroundColor: "rgba(2, 4, 7, 0)" }, { backgroundColor: "rgba(2, 4, 7, 0.72)" }], 220, "ease-out").then(() => (layer.style.backgroundColor = ""));
  }

  let drag = null;
  body.addEventListener(
    "touchstart",
    (event) => {
      drag = null;
      if (event.touches.length !== 1 || /^(INPUT|TEXTAREA|SELECT)$/.test(event.target.tagName)) return;
      // Из содержимого — только когда оно у начала: иначе это прокрутка.
      if (content.contains(event.target) && content.scrollTop > 0) return;
      drag = { y: event.touches[0].clientY, dy: 0, on: false, trail: [] };
    },
    { passive: true },
  );
  body.addEventListener(
    "touchmove",
    (event) => {
      if (!drag) return;
      const y = event.touches[0].clientY;
      const dy = y - drag.y;
      if (!drag.on) {
        if (dy < -4) return (drag = null);
        if (dy < 8) return;
        drag.on = true;
      }
      event.preventDefault();
      drag.dy = Math.max(0, dy - 8);
      drag.trail.push([event.timeStamp, y]);
      if (drag.trail.length > 6) drag.trail.shift();
      body.style.transform = `translateY(${drag.dy}px)`;
      layer.style.backgroundColor = `rgba(2, 4, 7, ${0.72 * Math.max(0, 1 - drag.dy / body.offsetHeight)})`;
    },
    { passive: false },
  );
  body.addEventListener("touchend", () => {
    const done = drag;
    drag = null;
    if (!done?.on) return;
    const first = done.trail[0];
    const last = done.trail[done.trail.length - 1];
    const speed = first && last && last[0] > first[0] ? (last[1] - first[1]) / (last[0] - first[0]) : 0;
    if (done.dy > body.offsetHeight * 0.3 || speed > 0.5) return close();
    play(body, [{ transform: body.style.transform }, { transform: "translateY(0)" }], 240).then(() => (body.style.transform = ""));
    play(layer, [{ backgroundColor: layer.style.backgroundColor }, { backgroundColor: "rgba(2, 4, 7, 0.72)" }], 240).then(() => (layer.style.backgroundColor = ""));
  });

  fill(content, close);
  return close;
}

/* ── Текст уроков ──────────────────────────────────────────────────────── */

const escapeHtml = (text) =>
  String(text).replace(/[&<>"']/g, (ch) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[ch]);

export function inline(text) {
  return escapeHtml(text)
    .replace(/`([^`]+)`/g, "<code>$1</code>")
    .replace(/\*\*([^*]+)\*\*/g, "<strong>$1</strong>")
    .replace(/(^|[^*])\*([^*\s][^*]*)\*/g, "$1<em>$2</em>")
    .replace(/\[([^\]]+)\]\([^)]+\)/g, "$1");
}

/** Подсветка блока кода: ключевые слова — пурпур, значения — циан, комментарии — тусклые. */
function highlight(line) {
  const comment = line.match(/^(.*?)(\s#.*|^#.*|\/\/.*)$/);
  let code = comment ? comment[1] : line;
  const tail = comment ? `<span class="cmt">${escapeHtml(comment[2])}</span>` : "";
  code = escapeHtml(code).replace(/^(\s*)([A-Z]{2,}|[a-z_-]+(?=:))/, '$1<span class="kw">$2</span>');
  code = code.replace(/(:\s+)([^\s].*)$/, '$1<span class="val">$2</span>');
  return code + tail;
}

function codeBlock(lang, lines) {
  const box = el("div", "code");
  const head = el("div", "code__head");
  head.append(el("span", "code__lang", lang || "код"));
  const copy = el("button", "code__copy");
  copy.append(icon("copy", 16, 2.25), "Копировать");
  copy.addEventListener("click", async () => {
    try {
      await navigator.clipboard.writeText(lines.join("\n"));
      toast("Скопировано");
    } catch {
      toast("Не скопировалось — выделите текст вручную");
    }
  });
  head.append(copy);
  const pre = el("pre");
  pre.innerHTML = lines.map(highlight).join("\n");
  box.append(head, pre);
  return box;
}

/**
 * Markdown урока в узлы: абзацы, списки, подзаголовки, код с «Копировать»,
 * цитата «Частая ошибка» — жёлтой рамкой, таблица — сеткой.
 */
export function markdown(source, className = "md") {
  const root = el("div", className);
  const lines = String(source ?? "").replace(/\r/g, "").split("\n");
  let i = 0;
  while (i < lines.length) {
    const line = lines[i];
    const fence = /^```\s*([\w+-]*)/.exec(line);
    if (fence) {
      const code = [];
      i++;
      while (i < lines.length && !/^```/.test(lines[i])) code.push(lines[i++]);
      i++;
      root.append(codeBlock(fence[1], code));
      continue;
    }
    if (!line.trim()) {
      i++;
      continue;
    }
    const head = /^(#{1,6})\s+(.*)$/.exec(line);
    if (head) {
      const node = el(head[1].length <= 3 ? "h3" : "h4");
      node.innerHTML = inline(head[2]);
      root.append(node);
      i++;
      continue;
    }
    if (/^>\s?/.test(line)) {
      const quote = [];
      while (i < lines.length && /^>\s?/.test(lines[i])) quote.push(lines[i++].replace(/^>\s?/, ""));
      const text = quote.join(" ");
      const warn = /^\**\s*(частая ошибка|ошибка|важно|внимание|осторожно)/i.exec(text);
      const box = el("div", warn ? "warn" : "note");
      if (warn) box.append(label(warn[1].replace(/^./, (c) => c.toUpperCase())));
      const body = el("span");
      body.innerHTML = inline(warn ? text.replace(/^\**\s*[^:*]+[:*]+\s*\**\s*/, "") : text);
      box.append(body);
      root.append(box);
      continue;
    }
    if (/^\s*([-*]|\d+[.)])\s+/.test(line)) {
      const ordered = /^\s*\d/.test(line);
      const list = el(ordered ? "ol" : "ul");
      while (i < lines.length && /^\s*([-*]|\d+[.)])\s+/.test(lines[i])) {
        // Вложенный пункт (с отступом) — сдвинут под родителя.
        const item = el("li", /^\s{2,}/.test(lines[i]) ? "md-sub" : "");
        item.innerHTML = inline(lines[i].replace(/^\s*([-*]|\d+[.)])\s+/, ""));
        list.append(item);
        i++;
      }
      root.append(list);
      continue;
    }
    if (/^\|/.test(line)) {
      const rows = [];
      while (i < lines.length && /^\|/.test(lines[i])) rows.push(lines[i++]);
      const table = el("div", "md-table");
      for (const row of rows) {
        if (/^\|[\s:|-]+\|$/.test(row)) continue;
        const cells = row.replace(/^\||\|$/g, "").split("|");
        const tr = el("div", "md-table__row");
        tr.style.gridTemplateColumns = `repeat(${cells.length}, minmax(0, 1fr))`;
        for (const cell of cells) {
          const td = el("span");
          td.innerHTML = inline(cell.trim());
          tr.append(td);
        }
        table.append(tr);
      }
      root.append(table);
      continue;
    }
    const para = [];
    while (i < lines.length && lines[i].trim() && !/^(```|#{1,6}\s|>|\s*([-*]|\d+[.)])\s+|\|)/.test(lines[i])) para.push(lines[i++]);
    const p = el("p");
    p.innerHTML = inline(para.join(" "));
    root.append(p);
  }
  return root;
}

/** Урок по разделам «## …» — так же, как делит его программа (tutor::sections). */
export function sections(lesson) {
  const parts = [];
  let currentPart = [];
  for (const line of String(lesson ?? "").replace(/\r/g, "").split("\n")) {
    if (line.startsWith("## ") && currentPart.some((l) => l.startsWith("## "))) {
      parts.push(currentPart.join("\n"));
      currentPart = [];
    }
    currentPart.push(line);
  }
  if (currentPart.join("").trim()) parts.push(currentPart.join("\n"));
  return parts.length ? parts : [String(lesson ?? "")];
}

export const sectionTitle = (part) => /^##\s+(.+)$/m.exec(part)?.[1]?.trim() ?? "";

/** Раздел без заголовков «# …» и «## …» — заголовок рисует экран. */
export const sectionBody = (part) =>
  part
    .split("\n")
    .filter((line) => !/^#{1,2}\s/.test(line))
    .join("\n");

/** Текст для голоса: без кода и разметки (код Ноа не зачитывает). */
export const speakable = (text) =>
  String(text ?? "")
    .replace(/```[\s\S]*?```/g, " ")
    .replace(/`[^`]*`/g, " ")
    .replace(/[#*>|_]/g, " ")
    .replace(/\s+/g, " ")
    .trim();

/* ── Кольцо Ноа ────────────────────────────────────────────────────────── */

// Холсты `canvas.noa-ring[data-ring]`: look — смотрит (дуга по кругу),
// listen — слушает, speak — говорит (20 крупных зубцов). Перенесено из макета.
let ringLoop = 0;
const ringStart = performance.now();

export function ring(mode = "look", size = 40) {
  const canvas = el("canvas", "noa-ring");
  canvas.dataset.ring = mode;
  canvas.dataset.size = String(size);
  canvas.style.width = `${size}px`;
  canvas.style.height = `${size}px`;
  if (!ringLoop) ringLoop = requestAnimationFrame(drawRings);
  return canvas;
}

function drawRings() {
  const reduce = window.matchMedia?.("(prefers-reduced-motion: reduce)").matches;
  const t = reduce ? 1.2 : (performance.now() - ringStart) / 1000;
  const list = document.querySelectorAll("canvas.noa-ring");
  for (const c of list) {
    const mode = c.dataset.ring;
    const S = Number(c.dataset.size) || 44;
    const dpr = window.devicePixelRatio || 1;
    if (c.width !== S * dpr) {
      c.width = S * dpr;
      c.height = S * dpr;
    }
    const x = c.getContext("2d");
    const k = S / 100;
    x.setTransform(dpr * k, 0, 0, dpr * k, 0, 0);
    x.clearRect(0, 0, 100, 100);
    const col = mode === "speak" ? ["#ff5ad2", "#b08cff", "#00f0ff"] : mode === "listen" ? ["#7dfcc6", "#00f0ff", "#7dfcc6"] : ["#00f0ff", "#7c8cf0", "#00f0ff"];
    const lvl = mode === "speak" ? 0.55 + Math.sin(t * 3.1) * 0.22 + Math.sin(t * 7.3) * 0.1 : 0.3 + Math.sin(t * 2.2) * 0.07;
    const g = x.createRadialGradient(50, 50, 2, 50, 50, 48);
    g.addColorStop(0, `${col[0]}40`);
    g.addColorStop(1, "transparent");
    x.fillStyle = g;
    x.beginPath();
    x.arc(50, 50, 48, 0, 6.3);
    x.fill();
    const sp = mode === "speak" ? 20 : 56;
    const base = 24 + lvl * 6;
    const amp = (mode === "speak" ? 13 : 6) * lvl + 1.5;
    x.beginPath();
    for (let i = 0; i <= sp; i++) {
      const a = (i / sp) * Math.PI * 2;
      const n =
        mode === "speak"
          ? Math.sin(a * 5 + t * 3) * 0.6 + Math.sin(a * 3 - t * 1.8) * 0.4
          : Math.sin(a * 9 + t * 4) * 0.5 + Math.sin(a * 5 - t * 2.6) * 0.5;
      const r = base + n * amp;
      const px = 50 + Math.cos(a) * r;
      const py = 50 + Math.sin(a) * r;
      if (i) x.lineTo(px, py);
      else x.moveTo(px, py);
    }
    x.closePath();
    const lg = x.createLinearGradient(20, 50, 80, 50);
    lg.addColorStop(0, col[0]);
    lg.addColorStop(0.5, col[1]);
    lg.addColorStop(1, col[2]);
    x.strokeStyle = lg;
    x.lineWidth = 2.4;
    x.shadowColor = col[0];
    x.shadowBlur = 12;
    x.stroke();
    x.shadowBlur = 0;
    if (mode === "look") {
      x.strokeStyle = `${col[0]}aa`;
      x.lineWidth = 1.5;
      x.beginPath();
      x.arc(50, 50, base + 12, t * 2, t * 2 + 1.2);
      x.stroke();
    }
  }
  ringLoop = requestAnimationFrame(drawRings);
}
