// Позиционирование попапа (SPEC §4).
//
// Тот же алгоритм продублирован на Rust в `src-tauri/src/overlay.rs` — там он работает
// в экранных координатах монитора, здесь в координатах вьюпорта. Константы обязаны
// совпадать, поэтому вынесены в именованные значения и продублированы в Rust как
// `GAP`/`INSET`. При правке одного места правьте оба.

export const GAP = 12; // зазор между окном и выделением
export const INSET = 12; // отступ от краёв экрана/вьюпорта
export const OFFSCREEN_MARGIN = 40; // запас, после которого выделение считается ушедшим

/**
 * @typedef {{left: number, right: number, top: number, bottom: number, width: number}} Anchor
 * @typedef {{width: number, height: number}} Size
 * @typedef {{x?: number, y?: number, width: number, height: number}} Viewport
 */

/**
 * Ставит окно над выделением, при нехватке места — под ним, и в любом случае
 * не даёт вылезти за экранный inset.
 * @param {{anchor: Anchor, size: Size, viewport: Viewport}} params
 * @returns {{left: number, top: number, flipped: boolean}}
 */
export function placePopup({ anchor, size, viewport }) {
  const vx = viewport.x ?? 0;
  const vy = viewport.y ?? 0;
  const vw = viewport.width;
  const vh = viewport.height;

  // По горизонтали — центрируем на выделении и прижимаем к inset.
  let left = anchor.left + anchor.width / 2 - size.width / 2;
  left = Math.max(vx + INSET, Math.min(left, vx + vw - size.width - INSET));

  // По вертикали — сначала пробуем над выделением.
  let top = anchor.top - GAP - size.height;
  let flipped = false;
  if (top < vy + INSET) {
    // Не помещается сверху — зеркалим под выделение.
    top = anchor.bottom + GAP;
    flipped = true;
  }

  // Финальный зажим в границы экрана. Нужен не только когда окно не помещается:
  // выделение может целиком уехать за верхний или нижний край (страницу
  // прокрутили), и тогда «под выделением» — это далеко за пределами экрана.
  const maxTop = Math.max(vy + INSET, vy + vh - size.height - INSET);
  top = Math.min(Math.max(top, vy + INSET), maxTop);

  return { left: Math.round(left), top: Math.round(top), flipped };
}

/**
 * Ширина окна с учётом узкого экрана: 400/480 обычно, `100vw − 24px` если не влезает.
 * @param {{expanded: boolean, viewportWidth: number, base?: number, expandedWidth?: number}} params
 */
export function popupWidth({ expanded, viewportWidth, base = 400, expandedWidth = 480 }) {
  return Math.min(expanded ? expandedWidth : base, viewportWidth - INSET * 2);
}

/**
 * Выделение всё ещё в пределах вьюпорта (с запасом)? Если нет — попап пора закрывать.
 * @param {Anchor} anchor @param {Viewport} viewport
 */
export function isAnchorVisible(anchor, viewport) {
  const vy = viewport.y ?? 0;
  return anchor.bottom >= vy - OFFSCREEN_MARGIN && anchor.top <= vy + viewport.height + OFFSCREEN_MARGIN;
}

export const MENU_PAD = 10; // отступ меню от краёв экрана
export const MENU_LIFT = 48; // насколько меню поднимается над выделением
export const MENU_DROP = 12; // насколько опускается, если сверху не поместилось

/**
 * Мини-меню ставится над НАЧАЛОМ выделения (первый rect): палец пользователя обычно
 * в конце выделения, и меню не должно оказаться под ним.
 * @param {{anchor: Anchor, size: Size, viewport: Viewport}} params
 */
export function placeMenu({ anchor, size, viewport }) {
  const vw = viewport.width;
  let left = anchor.left + anchor.width / 2 - size.width / 2;
  left = Math.max(MENU_PAD, Math.min(left, vw - size.width - MENU_PAD));

  let top = anchor.top - MENU_LIFT;
  if (top < MENU_PAD) top = anchor.bottom + MENU_DROP;

  return { left: Math.round(left), top: Math.round(top) };
}

/** Пустой rect (схлопнувшееся выделение) — не якорь. */
export function isEmptyRect(rect) {
  return !rect || (!rect.width && !rect.height);
}
