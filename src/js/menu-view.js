// Мини-меню выделения для мобильных (SPEC §3 Mobile).
//
// На Android и iOS системное меню выделения дополняется нативно (плагины в mobile/),
// но внутри собственного контента приложения — reader, in-app браузер, WebView —
// меню рисуем сами: так пункт «Объяснить» доступен и там, где ОС не даёт вклиниться
// в системное меню (актуально прежде всего для iOS, см. SPEC §9.5 и §12.2).

const TEMPLATE = `
<div class="menu" role="menu" aria-label="Действия с выделенным текстом">
  <button class="menu__btn" data-el="copy" type="button" role="menuitem" tabindex="-1">Копировать</button>
  <span class="menu__sep" aria-hidden="true"></span>
  <button class="menu__btn menu__btn--explain" data-el="explain" type="button" role="menuitem" tabindex="-1">
    <span class="menu__glyph" aria-hidden="true">?</span>Объяснить
  </button>
</div>`;

export class MenuView {
  /** @param {{onCopy: () => void, onExplain: () => void}} handlers */
  constructor(handlers) {
    const host = document.createElement("div");
    host.innerHTML = TEMPLATE.trim();
    this.el = host.firstElementChild;

    this.ui = {};
    for (const node of this.el.querySelectorAll("[data-el]")) this.ui[node.dataset.el] = node;

    // Меню не должно снимать выделение, ради которого оно и появилось.
    this.el.addEventListener("mousedown", (e) => e.preventDefault());
    this.el.addEventListener("touchstart", (e) => e.preventDefault(), { passive: false });

    this.ui.copy.addEventListener("click", (e) => {
      e.preventDefault();
      handlers.onCopy();
    });
    this.ui.explain.addEventListener("click", (e) => {
      e.preventDefault();
      handlers.onExplain();
    });
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
      // Отказ в разрешении на буфер обмена — молча, без ошибки в UI:
      // пользователь всё ещё может скопировать системным меню.
      return false;
    }
  }
  return false;
}
