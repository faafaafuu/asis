// Веб-хост попапа: триггер, жизненный цикл и позиционирование внутри одного документа
// (SPEC §3 Desktop, §4, §8).
//
// Используется в двух местах:
//   • демо-документ `demo.html` — проверка вёрстки и поведения без сборки Tauri;
//   • собственный контент приложения на мобильных (reader/WebView), где выделение
//     происходит в нашем же DOM и системные API не нужны.
//
// В настоящем десктопном приложении этот файл не работает: там выделение приходит из
// Rust экранными координатами, а попап — отдельное окно (см. `popup.html` и overlay.rs).

import { PopupView } from "./popup-view.js";
import { MenuView, copyText } from "./menu-view.js";
import { placePopup, placeMenu, isAnchorVisible, isEmptyRect } from "./position.js";

/** Пауза после изменения выделения на тач-устройствах: пока палец тянет — меню не мигает. */
const TOUCH_MENU_DELAY = 300;

export class WebHost {
  /**
   * @param {{
   *   client: { explain: Function, ask: Function },
   *   errorText?: string,
   *   requireLeftCtrl?: boolean,
   *   container?: HTMLElement,
   *   forceTouchMenu?: boolean,
   * }} opts
   */
  constructor(opts) {
    this.opts = opts;
    this.container = opts.container ?? document.body;
    this.requireLeftCtrl = opts.requireLeftCtrl ?? true;
    this.forceTouchMenu = opts.forceTouchMenu ?? false;

    this.open = false;
    this.anchor = null;
    this.range = null;
    this.leftCtrlDown = false;
    this.raf = 0;
    this.openTimer = 0;

    this.layer = document.createElement("div");
    this.layer.className = "popup-layer";
    this.layer.hidden = true;

    this.view = new PopupView({
      client: opts.client,
      errorText: opts.errorText,
      onGeometry: () => this.place(),
      onClose: () => this.hide(),
    });
    this.layer.append(this.view.el);

    // Мини-меню для тач-устройств.
    this.menuText = "";
    this.menuLayer = document.createElement("div");
    this.menuLayer.className = "menu-layer";
    this.menuLayer.hidden = true;
    this.menu = new MenuView({
      onCopy: () => {
        copyText(this.menuText);
        this.hideMenu();
      },
      onExplain: () => this.explainFromMenu(),
    });
    this.menuLayer.append(this.menu.el);
  }

  isTouch() {
    if (this.forceTouchMenu) return true;
    return Boolean(window.matchMedia?.("(pointer: coarse)").matches);
  }

  mount() {
    this.container.append(this.layer);
    this.container.append(this.menuLayer);

    this.onKeyDown = (e) => {
      // Различаем именно левый Ctrl: правый попап не открывает (SPEC §3, §12.5).
      if (e.code === "ControlLeft") this.leftCtrlDown = true;
      if (e.key === "Escape") {
        this.hideMenu();
        if (this.open) this.hide();
      }
    };
    this.onKeyUp = (e) => {
      if (e.code === "ControlLeft") this.leftCtrlDown = false;
    };
    this.onMouseUp = (e) => this.handleMouseUp(e);
    this.onDocDown = (e) => {
      if (!this.menuLayer.hidden && !this.menu.el.contains(e.target)) this.hideMenu();
      if (!this.open) return;
      if (this.view.el.contains(e.target)) return;
      this.hide();
    };
    this.onViewportChange = () => this.scheduleReposition();
    this.onSelectionChange = () => this.scheduleTouchMenu();

    // capture-фаза: Esc и клик снаружи должны срабатывать раньше обработчиков страницы.
    document.addEventListener("keydown", this.onKeyDown, true);
    document.addEventListener("keyup", this.onKeyUp, true);
    document.addEventListener("mousedown", this.onDocDown, true);
    document.addEventListener("mouseup", this.onMouseUp);
    document.addEventListener("selectionchange", this.onSelectionChange);
    window.addEventListener("scroll", this.onViewportChange, true);
    window.addEventListener("resize", this.onViewportChange);
    return this;
  }

  unmount() {
    document.removeEventListener("keydown", this.onKeyDown, true);
    document.removeEventListener("keyup", this.onKeyUp, true);
    document.removeEventListener("mousedown", this.onDocDown, true);
    document.removeEventListener("mouseup", this.onMouseUp);
    document.removeEventListener("selectionchange", this.onSelectionChange);
    window.removeEventListener("scroll", this.onViewportChange, true);
    window.removeEventListener("resize", this.onViewportChange);
    clearTimeout(this.openTimer);
    clearTimeout(this.menuTimer);
    cancelAnimationFrame(this.raf);
    this.view.close();
    this.layer.remove();
    this.menuLayer.remove();
  }

  handleMouseUp(e) {
    if (this.view.el.contains(e.target)) return; // клик внутри попапа
    if (this.menu.el.contains(e.target)) return; // клик внутри мини-меню
    if (e.target.closest?.("button, a, input, textarea, select, [role='button']")) return;

    // На тач-устройствах вход другой — мини-меню по selectionchange (SPEC §3 Mobile).
    if (this.isTouch()) return;
    if (this.requireLeftCtrl && !(e.ctrlKey && this.leftCtrlDown)) return;

    // Один тик задержки: Selection API фиксирует финальное состояние после mouseup.
    clearTimeout(this.openTimer);
    this.openTimer = setTimeout(() => this.openFromSelection(), 0);
  }

  /** Открытие по текущему выделению документа. */
  openFromSelection() {
    const sel = window.getSelection();
    const text = sel ? sel.toString().trim() : "";
    if (!text || !sel.rangeCount) {
      if (this.open) this.hide();
      return;
    }
    this.range = sel.getRangeAt(0).cloneRange();
    const anchor = anchorFromRange(this.range);
    if (!anchor) return;
    this.showAt(anchor, text);
  }

  /**
   * Показ попапа у готового якоря. Повторное открытие при уже открытом окне —
   * без анимации закрытия, просто новый якорь и сброс состояния (SPEC §8).
   */
  showAt(anchor, term, context = "") {
    this.anchor = anchor;
    this.open = true;
    this.layer.hidden = false;
    // Замер скрытым: пока позиция не посчитана, окно не должно быть видно (SPEC §4).
    this.layer.style.visibility = "hidden";
    this.placed = false;
    this.view.open({ term, context });
  }

  hide() {
    if (!this.open) return;
    this.open = false;
    this.anchor = null;
    this.range = null;
    this.layer.hidden = true;
    this.view.close();
  }

  /* ── Мини-меню на тач-устройствах ──────────────────────────────────────── */

  scheduleTouchMenu() {
    if (!this.isTouch()) return;
    clearTimeout(this.menuTimer);
    this.menuTimer = setTimeout(() => this.showTouchMenu(), TOUCH_MENU_DELAY);
  }

  showTouchMenu() {
    const sel = window.getSelection();
    const text = sel ? sel.toString().trim() : "";
    if (!text || !sel.rangeCount) {
      this.hideMenu();
      return;
    }
    // Пока открыт попап, меню не нужно: пользователь уже получает объяснение.
    if (this.open) return;

    this.range = sel.getRangeAt(0).cloneRange();
    this.menuText = text;

    const anchor = anchorFromRange(this.range, "first");
    if (!anchor) return;

    this.menuLayer.hidden = false;
    const { left, top } = placeMenu({
      anchor,
      size: { width: this.menu.el.offsetWidth, height: this.menu.el.offsetHeight },
      viewport: { width: window.innerWidth, height: window.innerHeight },
    });
    this.menuLayer.style.left = `${left}px`;
    this.menuLayer.style.top = `${top}px`;
  }

  hideMenu() {
    clearTimeout(this.menuTimer);
    this.menuLayer.hidden = true;
  }

  /** «Объяснить» из меню: попап встаёт у конца выделения, как и на десктопе. */
  explainFromMenu() {
    const text = this.menuText;
    this.hideMenu();
    if (!text || !this.range) return;
    const anchor = anchorFromRange(this.range);
    if (!anchor) return;
    this.showAt(anchor, text);
  }

  place() {
    if (!this.open || !this.anchor) return;
    const { left, top } = placePopup({
      anchor: this.anchor,
      size: { width: this.view.el.offsetWidth, height: this.view.el.offsetHeight },
      viewport: { width: window.innerWidth, height: window.innerHeight },
    });
    this.layer.style.left = `${left}px`;
    this.layer.style.top = `${top}px`;
    this.layer.style.visibility = "visible";
    this.placed = true;
  }

  /** Скролл/ресайз: пересчёт позиции по rAF, закрытие при уходе выделения с экрана. */
  scheduleReposition() {
    if (!this.open || this.raf) return;
    this.raf = requestAnimationFrame(() => {
      this.raf = 0;
      if (!this.open) return;
      if (this.range) {
        const anchor = anchorFromRange(this.range);
        if (!anchor) return;
        if (!isAnchorVisible(anchor, { width: window.innerWidth, height: window.innerHeight })) {
          this.hide();
          return;
        }
        this.anchor = anchor;
      }
      this.place();
    });
  }
}

/**
 * Якорь — ПОСЛЕДНИЙ прямоугольник выделения, а не bounding box: для многострочного
 * выделения окно должно вставать у конца текста, а не у геометрического центра (SPEC §4).
 * Мини-меню, наоборот, привязывается к первому rect — к началу выделения.
 * @param {Range} range @param {'first'|'last'} [which]
 */
export function anchorFromRange(range, which = "last") {
  const rects = range.getClientRects();
  const fallback = range.getBoundingClientRect();
  const rect = rects.length ? (which === "first" ? rects[0] : rects[rects.length - 1]) : fallback;
  if (isEmptyRect(rect)) return null;
  return { left: rect.left, right: rect.right, top: rect.top, bottom: rect.bottom, width: rect.width };
}
