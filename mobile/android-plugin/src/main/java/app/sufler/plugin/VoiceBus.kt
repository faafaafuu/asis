package app.sufler.plugin

/**
 * Просьба «слушай» от жеста ассистента, ярлыка или плитки — тот же приём, что
 * у [SelectionBus]: приложение могло быть не запущено, и тогда просьба ждёт,
 * пока экран разговора не спросит о ней командой `pendingVoice`.
 */
object VoiceBus {

    @Volatile
    var listener: (() -> Unit)? = null

    @Volatile
    private var pending = false

    @Synchronized
    fun request() {
        pending = true
        listener?.invoke()
    }

    /** Была ли просьба — ровно один раз. */
    @Synchronized
    fun takePending(): Boolean {
        val value = pending
        pending = false
        return value
    }
}
