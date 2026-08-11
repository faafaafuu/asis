package app.sufler.plugin

/**
 * Мостик между активностью-приёмником и плагином Tauri.
 *
 * Нужен потому, что порядок не гарантирован: пункт меню может сработать, когда
 * приложение ещё не запущено и WebView не создан. Тогда текст ждёт здесь, а плагин
 * заберёт его командой `pendingSelection` сразу после загрузки.
 */
object SelectionBus {

    /**
     * Слушатель ставится плагином, когда WebView готов принимать события.
     *
     * Отложенный текст здесь намеренно НЕ отдаётся. Плагин ставит слушателя,
     * как только создан WebView, — но страница в этот момент ещё грузится и на
     * события не подписана, так что отданный сразу текст улетал в никуда.
     * А поскольку отдача его вычёрпывала, до `pendingSelection` он тоже не
     * доживал: пункт «Объяснить» на незапущенном приложении не показывал
     * ничего. Текст ждёт, пока фронтенд не заберёт его сам.
     */
    @Volatile
    var listener: ((String) -> Unit)? = null

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
