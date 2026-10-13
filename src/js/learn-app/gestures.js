// Жесты, как у приложений телефона: смахнуть строку влево — действия
// («Удалить»), до конца — сразу первое; удержать карточку — меню над
// размытым экраном; потянуть список вниз — обновить. Подтверждение
// удаления — лист действий снизу. На компьютере меню — правой кнопкой.

import { el, icon, haptic, still, EASE } from "./core.js";

/* ── Смахнуть строку ─────────────────────────────────────────────────────── */

/** Открытая строка: одна на экран, касание мимо её закрывает. */
let opened = null;
document.addEventListener(
  "touchstart",
  (event) => {
    if (opened && !opened.wrap.contains(event.target)) opened.close();
  },
  { passive: true, capture: true },
);

const ACTION = 84;

/**
 * Обёртка строки с действиями под ней. `actions` — `{ text, icon, danger,
 * run }`; первое — основное: на него — смахивание до конца. Отдаёт узел,
 * который кладут в список вместо самой строки.
 */
export function swipeRow(front, actions) {
  const wrap = el("div", "swipe");
  const under = el("div", "swipe__actions");
  const width = ACTION * actions.length;
  for (const action of actions) {
    const btn = el("button", `swipe__act${action.danger ? " is-danger" : ""}`);
    btn.append(icon(action.icon ?? "trash", 20, 2), el("span", "", action.text));
    btn.addEventListener("click", (event) => {
      event.stopPropagation();
      close();
      action.run();
    });
    under.append(btn);
  }
  front.classList.add("swipe__front");
  wrap.append(under, front);

  let x = 0;
  let drag = null;
  let armed = false;
  let swallow = false;

  const set = (to, animate = false) => {
    x = to;
    const ease = animate && !still() ? `${EASE}` : "";
    front.style.transition = ease ? `transform 0.3s ${ease}` : "none";
    under.style.transition = ease ? `width 0.3s ${ease}` : "none";
    front.style.transform = to ? `translateX(${to}px)` : "";
    under.style.width = `${Math.max(0, -to)}px`;
  };
  const close = () => {
    if (opened?.wrap === wrap) opened = null;
    set(0, true);
  };

  wrap.addEventListener(
    "touchstart",
    (event) => {
      drag = null;
      if (event.touches.length !== 1) return;
      const touch = event.touches[0];
      drag = { x: touch.clientX, y: touch.clientY, base: x, on: false, trail: [] };
    },
    { passive: true },
  );
  wrap.addEventListener(
    "touchmove",
    (event) => {
      if (!drag) return;
      const touch = event.touches[0];
      const dx = touch.clientX - drag.x;
      const dy = touch.clientY - drag.y;
      if (!drag.on) {
        if (Math.abs(dy) > 10 && Math.abs(dy) > Math.abs(dx)) {
          drag = null;
          return;
        }
        if (Math.abs(dx) < 10) return;
        // Закрытую строку тянут только влево; вправо — свайп «назад».
        if (!drag.base && dx > 0) {
          drag = null;
          return;
        }
        drag.on = true;
        if (opened && opened.wrap !== wrap) opened.close();
      }
      event.preventDefault();
      drag.trail.push([event.timeStamp, touch.clientX]);
      if (drag.trail.length > 6) drag.trail.shift();
      let to = drag.base + dx;
      if (to > 0) to = to * 0.2;
      set(to);
      const full = -to > wrap.offsetWidth * 0.6;
      if (full !== armed) {
        armed = full;
        wrap.classList.toggle("is-armed", armed);
        if (armed) haptic("medium");
      }
    },
    { passive: false },
  );
  const end = () => {
    const done = drag;
    drag = null;
    if (!done?.on) return;
    swallow = true;
    setTimeout(() => (swallow = false), 350);
    const first = done.trail[0];
    const last = done.trail[done.trail.length - 1];
    const speed = first && last && last[0] > first[0] ? (last[1] - first[1]) / (last[0] - first[0]) : 0;
    if (armed) {
      armed = false;
      wrap.classList.remove("is-armed");
      set(-wrap.offsetWidth, true);
      opened = null;
      actions[0].run();
      return;
    }
    if ((-x > width / 2 && speed < 0.3) || speed < -0.4) {
      set(-width, true);
      opened = { wrap, close };
    } else close();
  };
  wrap.addEventListener("touchend", end);
  wrap.addEventListener("touchcancel", end);
  // После смахивания тап по строке не открывает её; открытую — закрывает.
  front.addEventListener(
    "click",
    (event) => {
      if (swallow || x) {
        event.stopImmediatePropagation();
        event.preventDefault();
        if (x && !swallow) close();
      }
    },
    true,
  );
  wrap.close = close;
  return wrap;
}

/** Строка уходит: схлопывается по высоте, потом удаляется. */
export function collapse(node) {
  if (still() || !node.isConnected) {
    node.remove();
    return Promise.resolve();
  }
  const height = node.offsetHeight;
  node.style.overflow = "hidden";
  return node
    .animate(
      [
        { height: `${height}px`, opacity: 1 },
        { height: "0px", opacity: 0, marginTop: "0px", marginBottom: "0px" },
      ],
      { duration: 260, easing: EASE, fill: "forwards" },
    )
    .finished.then(
      () => node.remove(),
      () => node.remove(),
    );
}

/* ── Удержать — меню ─────────────────────────────────────────────────────── */

const HOLD = 420;

/**
 * Меню по удержанию: карточка приподнимается над размытым экраном, под ней —
 * действия. `items()` — `[{ text, icon, danger, run }]`, собираются в момент
 * открытия. Обычный тап работает как раньше.
 */
export function pressMenu(target, items) {
  target.classList.add("pressable");
  let timer = 0;
  let start = null;
  let fired = false;
  const cancel = () => {
    clearTimeout(timer);
    target.classList.remove("is-holding");
    start = null;
  };
  target.addEventListener(
    "touchstart",
    (event) => {
      fired = false;
      if (event.touches.length !== 1) return;
      start = { x: event.touches[0].clientX, y: event.touches[0].clientY };
      target.classList.add("is-holding");
      timer = setTimeout(() => {
        target.classList.remove("is-holding");
        if (!start) return;
        fired = true;
        start = null;
        openMenu(target, items());
      }, HOLD);
    },
    { passive: true },
  );
  target.addEventListener(
    "touchmove",
    (event) => {
      if (!start) return;
      const touch = event.touches[0];
      if (Math.hypot(touch.clientX - start.x, touch.clientY - start.y) > 8) cancel();
    },
    { passive: true },
  );
  target.addEventListener("touchend", (event) => {
    cancel();
    if (fired && event.cancelable) event.preventDefault();
  });
  target.addEventListener("touchcancel", cancel);
  target.addEventListener(
    "click",
    (event) => {
      if (!fired) return;
      fired = false;
      event.stopImmediatePropagation();
      event.preventDefault();
    },
    true,
  );
  // Правая кнопка мыши; на Android — тоже долгое нажатие (не второй раз).
  target.addEventListener("contextmenu", (event) => {
    event.preventDefault();
    if (fired || document.querySelector(".ctx")) return;
    openMenu(target, items());
  });
}

function openMenu(target, items) {
  if (!items?.length || document.querySelector(".ctx")) return;
  haptic("medium");
  const rect = target.getBoundingClientRect();
  const layer = el("div", "ctx");
  const shade = el("div", "ctx__shade");
  const ghost = target.cloneNode(true);
  ghost.classList.remove("is-holding", "swipe__front");
  ghost.classList.add("ctx__ghost");
  ghost.style.cssText = `left:${rect.left}px;top:${rect.top}px;width:${rect.width}px;height:${rect.height}px;transform:none`;
  ghost.setAttribute("aria-hidden", "true");
  const menu = el("div", "ctx__menu");
  menu.setAttribute("role", "menu");
  let closing = false;
  const close = () => {
    if (closing) return Promise.resolve();
    closing = true;
    layer.style.pointerEvents = "none";
    if (still()) {
      layer.remove();
      return Promise.resolve();
    }
    menu.animate([{ opacity: 1, transform: "scale(1)" }, { opacity: 0, transform: "scale(0.9)" }], { duration: 150, fill: "forwards" });
    ghost.animate([{ transform: "scale(1.03)", top: ghost.style.top }, { transform: "scale(1)", top: `${rect.top}px` }], { duration: 180, easing: EASE, fill: "forwards" });
    return shade
      .animate([{ opacity: 1 }, { opacity: 0 }], { duration: 180, fill: "forwards" })
      .finished.then(
        () => layer.remove(),
        () => layer.remove(),
      );
  };
  for (const item of items) {
    const btn = el("button", `ctx__item${item.danger ? " is-danger" : ""}`);
    btn.setAttribute("role", "menuitem");
    btn.append(el("span", "grow", item.text), icon(item.icon ?? "chevron-right", 20, 2));
    btn.addEventListener("click", () => {
      haptic("light");
      close().then(() => item.run());
    });
    menu.append(btn);
  }
  shade.addEventListener("click", () => close());
  layer.append(shade, ghost, menu);
  document.body.append(layer);

  // Меню — под карточкой; не влезает — над ней; высокая карточка — сдвигается вверх.
  const view = window.visualViewport;
  const viewTop = (view?.offsetTop ?? 0) + 12 + safeTop();
  const viewBottom = (view?.offsetTop ?? 0) + (view?.height ?? window.innerHeight) - 12 - safeBottom();
  const menuHeight = menu.offsetHeight;
  let ghostTop = rect.top;
  let menuTop = rect.bottom + 10;
  let origin = "top";
  if (menuTop + menuHeight > viewBottom) {
    if (rect.top - 10 - menuHeight >= viewTop) {
      menuTop = rect.top - 10 - menuHeight;
      origin = "bottom";
    } else {
      ghostTop = Math.max(viewTop, viewBottom - menuHeight - 10 - rect.height);
      menuTop = Math.min(ghostTop + rect.height + 10, viewBottom - menuHeight);
    }
  }
  const width = Math.min(260, window.innerWidth - 24);
  const left = Math.min(Math.max(12, rect.left), window.innerWidth - width - 12);
  menu.style.cssText = `top:${menuTop}px;left:${left}px;width:${width}px;transform-origin:${origin} left`;
  if (still()) {
    ghost.style.top = `${ghostTop}px`;
    return;
  }
  shade.animate([{ opacity: 0 }, { opacity: 1 }], { duration: 200, easing: "ease-out" });
  ghost.animate([{ transform: "scale(0.97)", top: `${rect.top}px` }, { transform: "scale(1.03)", top: `${ghostTop}px` }], { duration: 280, easing: EASE, fill: "forwards" });
  ghost.style.top = `${ghostTop}px`;
  menu.animate([{ opacity: 0, transform: "scale(0.85)" }, { opacity: 1, transform: "scale(1)" }], { duration: 260, easing: EASE });
}

/** Вырез сверху и полоска «домой» снизу — как их отступил каркас. */
const padOf = (selector, side) => {
  const node = document.querySelector(selector);
  return node ? parseFloat(getComputedStyle(node)[side]) || 0 : 0;
};
const safeTop = () => padOf(".app", "paddingTop");
const safeBottom = () => padOf(".tabs", "paddingBottom");

/* ── Подтверждение — лист действий снизу ─────────────────────────────────── */

/** «Удалить …?» — красная кнопка и «Отмена» снизу экрана. Отдаёт true/false. */
export function confirmAction({ title = "", text = "", ok = "Удалить", cancel = "Отмена" } = {}) {
  return new Promise((resolve) => {
    const layer = el("div", "act");
    const box = el("div", "act__box");
    const group = el("div", "act__group");
    if (title || text) {
      const head = el("div", "act__head");
      if (title) head.append(el("span", "act__title", title));
      if (text) head.append(el("span", "act__text", text));
      group.append(head);
    }
    const yes = el("button", "act__btn is-danger", ok);
    group.append(yes);
    const no = el("button", "act__btn act__cancel", cancel);
    box.append(group, no);
    layer.append(box);
    document.body.append(layer);
    let done = false;
    const finish = (value) => {
      if (done) return;
      done = true;
      layer.style.pointerEvents = "none";
      if (value) haptic("warning");
      resolve(value);
      if (still()) return layer.remove();
      box.animate([{ transform: "translateY(0)" }, { transform: "translateY(110%)" }], { duration: 200, easing: "cubic-bezier(0.4, 0, 1, 1)", fill: "forwards" });
      layer
        .animate([{ opacity: 1 }, { opacity: 0 }], { duration: 200, fill: "forwards" })
        .finished.then(
          () => layer.remove(),
          () => layer.remove(),
        );
    };
    yes.addEventListener("click", () => finish(true));
    no.addEventListener("click", () => finish(false));
    layer.addEventListener("click", (event) => {
      if (event.target === layer) finish(false);
    });
    if (!still()) {
      layer.animate([{ opacity: 0 }, { opacity: 1 }], { duration: 180, easing: "ease-out" });
      box.animate([{ transform: "translateY(100%)" }, { transform: "translateY(0)" }], { duration: 300, easing: EASE });
    }
  });
}

/* ── Потянуть вниз — обновить ────────────────────────────────────────────── */

const PULL = 64;

/**
 * На прокручиваемом `content`: у начала списка тянут вниз — появляется
 * кольцо, дотянули — отклик и «отпустите»; отпустили — `run()`, пока он
 * идёт, кольцо крутится.
 */
export function pullToRefresh(content, run) {
  const tip = el("div", "ptr");
  const spin = el("span", "ptr__spin");
  const text = el("span", "ptr__text", "Потяните — обновить");
  tip.append(spin, text);
  content.before(tip);
  let drag = null;
  let busy = false;

  // Полоска над списком растёт — список уезжает вниз вместе с ней.
  const place = (pull) => {
    tip.style.height = `${pull}px`;
    tip.style.opacity = String(Math.min(1, pull / PULL));
    spin.style.transform = `rotate(${Math.round((pull / PULL) * 270)}deg)`;
  };
  const settle = (pull) => {
    tip.style.transition = still() ? "none" : `height 0.28s ${EASE}, opacity 0.2s`;
    place(pull);
    setTimeout(() => (tip.style.transition = ""), 300);
  };

  content.addEventListener(
    "touchstart",
    (event) => {
      drag = null;
      if (busy || event.touches.length !== 1 || content.scrollTop > 0) return;
      drag = { y: event.touches[0].clientY, x: event.touches[0].clientX, on: false, pull: 0, armed: false };
    },
    { passive: true },
  );
  content.addEventListener(
    "touchmove",
    (event) => {
      if (!drag) return;
      const dy = event.touches[0].clientY - drag.y;
      const dx = event.touches[0].clientX - drag.x;
      if (!drag.on) {
        if (dy < 0 || Math.abs(dx) > Math.abs(dy)) {
          drag = null;
          return;
        }
        if (dy < 6 || content.scrollTop > 0) return;
        drag.on = true;
      }
      event.preventDefault();
      // Тянется туже, чем палец: как резинка.
      drag.pull = Math.max(0, Math.min(120, (dy - 6) * 0.5));
      place(drag.pull);
      const armed = drag.pull >= PULL;
      if (armed !== drag.armed) {
        drag.armed = armed;
        tip.classList.toggle("is-armed", armed);
        text.textContent = armed ? "Отпустите — обновить" : "Потяните — обновить";
        if (armed) haptic("light");
      }
    },
    { passive: false },
  );
  const end = async () => {
    const done = drag;
    drag = null;
    if (!done?.on) return;
    tip.classList.remove("is-armed");
    if (!done.armed) return settle(0);
    busy = true;
    tip.classList.add("is-busy");
    text.textContent = "Обновляю…";
    settle(48);
    try {
      await run();
    } finally {
      busy = false;
      tip.classList.remove("is-busy");
      if (tip.isConnected) settle(0);
    }
  };
  content.addEventListener("touchend", end);
  content.addEventListener("touchcancel", end);
}
