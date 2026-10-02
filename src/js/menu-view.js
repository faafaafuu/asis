// Мини-меню выделения для мобильных.
//
// На Android и iOS системное меню выделения дополняется нативно (плагины в mobile/),
// но внутри собственного контента приложения — reader, in-app браузер, WebView —
// меню рисуем сами: так пункт «Объяснить» доступен и там, где ОС не даёт вклиниться
// в системное меню (актуально прежде всего для iOS).

const TEMPLATE = `
<div class="menu menu--icons" role="menu" aria-label="Действия с выделенным текстом">
  <button class="menu__btn" data-el="copy" type="button" role="menuitem" tabindex="-1"
          title="Копировать" aria-label="Копировать">⧉</button>
  <button class="menu__btn" data-el="read" type="button" role="menuitem" tabindex="-1"
          title="Прочитать вслух" aria-label="Прочитать вслух" hidden>🔊</button>
  <span data-el="readSep" hidden></span>
  <button class="menu__btn menu__btn--explain" data-el="explain" type="button" role="menuitem" tabindex="-1"
          title="Объяснить" aria-label="Объяснить"><span class="menu__glyph">?</span></button>
</div>`;

export class MenuView {
  /** @param {{onCopy: () => void, onExplain: () => void, onRead?: () => void}} handlers */
  constructor(handlers) {
    const host = document.createElement("div");
    host.innerHTML = TEMPLATE.trim();
    this.el = host.firstElementChild;

    this.ui = {};
    for (const node of this.el.querySelectorAll("[data-el]")) this.ui[node.dataset.el] = node;

    // Меню не должно снимать выделение, ради которого оно и появилось. Только
    // для мыши: отменённый touchstart на телефоне глотает и сам тап — кнопки
    // молчали. Пальцем выделение может и сняться, но текст меню уже запомнило.
    this.el.addEventListener("mousedown", (e) => e.preventDefault());
    this.el.addEventListener("pointerdown", (e) => {
      this.pressed = true;
      if (e.pointerType === "mouse") e.preventDefault();
    });
    const release = () => setTimeout(() => (this.pressed = false), 0);
    this.el.addEventListener("pointerup", release);
    this.el.addEventListener("pointercancel", release);

    this.ui.copy.addEventListener("click", (e) => {
      e.preventDefault();
      handlers.onCopy();
    });
    // «Прочитать» — там, где есть чем читать вслух. Пока звучит, кнопка —
    // «стоп»: без неё длинное выделение было не остановить.
    if (handlers.onRead) {
      this.ui.read.hidden = false;
      this.ui.readSep.hidden = false;
      this.ui.read.addEventListener("click", (e) => {
        e.preventDefault();
        if (this.reading) {
          handlers.onStopRead?.();
          this.setReading(false);
          return;
        }
        this.setReading(true);
        Promise.resolve(handlers.onRead()).finally(() => this.setReading(false));
      });
    }
    this.ui.explain.addEventListener("click", (e) => {
      e.preventDefault();
      handlers.onExplain();
    });
  }

  setReading(on) {
    this.reading = on;
    this.ui.read.textContent = on ? "■" : "🔊";
    this.ui.read.title = this.ui.read.ariaLabel = on ? "Остановить" : "Прочитать вслух";
  }

  /** Короткий знак на кнопке — «сделано» или «не вышло». */
  flash(name, glyph) {
    const button = this.ui[name];
    const was = button.textContent;
    button.textContent = glyph;
    setTimeout(() => (button.textContent = was), 700);
  }
}

/** Копирование выделенного текста. Возвращает промис — вызывающий решает, что делать с отказом. */
export async function copyText(text) {
  if (!text) return false;
  if (navigator.clipboard?.writeText) {
    try {
      await navigator.clipboard.writeText(text);
      return true;
    } catch {
      // Отказ в разрешении на буфер обмена — пробуем по-старому ниже.
    }
  }
  // Старый путь: незащищённая страница, WebView, отказ в разрешении.
  const area = document.createElement("textarea");
  area.value = text;
  area.setAttribute("readonly", "");
  area.style.cssText = "position:fixed;top:0;left:0;opacity:0;pointer-events:none";
  document.body.append(area);
  area.select();
  let ok = false;
  try {
    ok = document.execCommand("copy");
  } catch {
    ok = false;
  }
  area.remove();
  return ok;
}
