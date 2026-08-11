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
      // Подписка не удалась — но это не повод не забрать текст, который уже
      // ждёт. Раньше здесь стоял return, и одна неудачная подписка отменяла
      // единственный путь, работающий при холодном запуске: пункт «Объяснить»
      // на незапущенном приложении молчал.
      console.debug("подписка на выделение не удалась:", err);
    }
  }

  // Текст, пришедший до подписки. Именно этим путём приходит выделение, когда
  // пункт меню запускает приложение с нуля, — то есть в большинстве случаев.
  try {
    const pending = await api.invoke(`plugin:${PLUGIN}|pendingSelection`);
    const text = String(pending?.text ?? "").trim();
    if (text) onSelection(text);
  } catch {
    // Команды нет — десктоп. Молча пропускаем.
  }
}
