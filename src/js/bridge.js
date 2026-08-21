// Мост к Tauri. Фронтенд попапа собран без бандлера, поэтому API берётся из
// глобального объекта (`withGlobalTauri: true` в tauri.conf.json), а не из npm-пакета:
// одна зависимость меньше, и окно попапа грузится без единого сетевого запроса.

/** @returns {{invoke: Function, listen: Function} | null} null — значит мы в обычном браузере. */
export function tauri() {
  const api = globalThis.__TAURI__;
  if (!api?.core?.invoke) return null;
  return {
    invoke: (cmd, args) => api.core.invoke(cmd, args),
    listen: (event, handler) => api.event.listen(event, handler),
  };
}

/**
 * Само окно приложения — для того, что делает система, а не мы.
 *
 * Перетаскивание и растягивание окна webview начать может, но вести их обязана
 * система: она держит курсор до отпускания кнопки, знает про края экрана и про
 * прилипание. Наш код только говорит «началось».
 *
 * @returns {{startDragging: Function, startResizeDragging: Function} | null}
 */
export function appWindow() {
  const api = globalThis.__TAURI__;
  try {
    return api?.window?.getCurrentWindow?.() ?? null;
  } catch {
    return null;
  }
}

/**
 * Тема окна. `system` разрешается здесь, а не в CSS: Rust уже знает системную
 * настройку, но в вебе её приходится спрашивать у медиазапроса.
 */
export function applyTheme(theme) {
  document.documentElement.dataset.theme = theme ?? "system";
}
