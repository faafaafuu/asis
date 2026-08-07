package app.sufler.plugin

import android.app.Activity
import android.webkit.WebView
import app.tauri.annotation.Command
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.JSObject
import app.tauri.plugin.Invoke
import app.tauri.plugin.Plugin

/**
 * Плагин Tauri для Android (SPEC §9.4).
 *
 * Отдаёт фронтенду текст, выбранный пользователем через системный пункт меню
 * «Объяснить», двумя путями:
 *   • событие `selection` — когда приложение уже запущено и WebView готов;
 *   • команда `pendingSelection` — когда пункт меню запустил приложение с нуля
 *     и текст пришёл раньше, чем фронтенд успел подписаться.
 */
@TauriPlugin
class SuflerPlugin(private val activity: Activity) : Plugin(activity) {

    override fun load(webView: WebView) {
        super.load(webView)
        SelectionBus.listener = { text ->
            val payload = JSObject()
            payload.put("text", text)
            trigger("selection", payload)
        }
    }

    /** Забрать текст, пришедший до готовности фронтенда. Возвращает пустую строку, если его нет. */
    @Command
    fun pendingSelection(invoke: Invoke) {
        val result = JSObject()
        result.put("text", SelectionBus.takePending().orEmpty())
        invoke.resolve(result)
    }

    /**
     * Проверка, что системная точка входа вообще доступна.
     *
     * ACTION_PROCESS_TEXT поддерживают почти все текстовые поля, но не все: приложения
     * с полностью кастомной отрисовкой текста (часть игр, некоторые ридеры) своё меню
     * выделения не отдают системе. Честно сообщаем об этом, а не делаем вид, что
     * работаем везде.
     */
    @Command
    fun integrationStatus(invoke: Invoke) {
        val result = JSObject()
        result.put("kind", "ready")
        result.put(
            "hint",
            "Выделите текст в любом приложении и выберите «Объяснить» в меню выделения."
        )
        invoke.resolve(result)
    }
}
