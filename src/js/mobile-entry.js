// Мобильная точка входа: текст приходит не от системного хука, а от нативного
// плагина — пункта «Объяснить» в меню выделения (Android, SPEC §9.4) либо из
// Share Extension (iOS, SPEC §9.5).
//
// TODO: сквозная проверка требует реального устройства и сгенерированного проекта
// (`tauri android init` / `tauri ios init`) — ни Android Studio, ни Xcode в окружении,
// где писался этот код, не было. Логика фронтенда здесь полная, но связка
// «нативный плагин → событие → попап» на устройстве не прогонялась.

const PLUGIN = "sufler";

/**
 * Подписывается на выделения от нативного плагина и забирает текст, который мог
 * прийти до готовности фронтенда (пункт меню запускает приложение с нуля).
 *
 * @param {{invoke: Function}} api мост к Tauri
 * @param {(text: string) => void} onSelection
 */
export async function attachMobileEntry(api, onSelection) {
    const listener = globalThis.__TAURI__?.core?.addPluginListener;
    if (typeof listener === "function") {
    try {
      await listener(PLUGIN, "selection", (payload) => {
        const text = String(payload?.text ?? "").trim();
        if (text) onSelection(text);
      });
    } catch (err) {
      // Плагина нет — значит это десктопная сборка, и это нормально.
      console.debug("плагин выделения недоступен:", err);
      return;
    }
  }

  // Текст, пришедший до подписки.
  try {
    const pending = await api.invoke(`plugin:${PLUGIN}|pendingSelection`);
    const text = String(pending?.text ?? "").trim();
    if (text) onSelection(text);
  } catch {
    // Команды нет — десктоп. Молча пропускаем.
  }
}
