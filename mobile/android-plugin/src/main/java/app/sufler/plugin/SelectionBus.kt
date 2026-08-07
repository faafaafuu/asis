package app.sufler.plugin

/**
 * Мостик между активностью-приёмником и плагином Tauri.
 *
 * Нужен потому, что порядок не гарантирован: пункт меню может сработать, когда
 * приложение ещё не запущено и WebView не создан. Тогда текст ждёт здесь, а плагин
 * заберёт его командой `pendingSelection` сразу после загрузки.
 */
object SelectionBus {

    /** Слушатель ставится плагином, когда WebView готов принимать события. */
    @Volatile
    var listener: ((String) -> Unit)? = null
        set(value) {
            field = value
            // Если текст пришёл раньше подписки — отдаём его немедленно.
            val waiting = takePending()
            if (value != null && waiting != null) value(waiting)
        }

    @Volatile
    private var pending: String? = null

    @Synchronized
    fun publish(text: String) {
        val current = listener
        if (current != null) {
            current(text)
        } else {
            pending = text
        }
    }

    /** Забирает отложенный текст ровно один раз. */
    @Synchronized
    fun takePending(): String? {
        val value = pending
        pending = null
        return value
    }
}
