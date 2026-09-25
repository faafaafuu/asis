package app.sufler.plugin

import android.app.Activity
import android.os.Bundle

/**
 * Запрос разрешения — своей непрозрачной активностью, а не поверх Ноа.
 *
 * Если системное окно разрешения открыто прямо над WebView и человек в этот
 * момент уходит на главный экран, WebView падает (SIGILL в onTrimMemory,
 * WebView 145). Когда Ноа закрыт этой активностью целиком, WebView уходит в
 * фон обычным путём, как по кнопке «Домой», — и это он переживает.
 */
class PermissionActivity : Activity() {

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val wanted = intent.getStringArrayExtra(EXTRA_PERMISSIONS)
        if (wanted.isNullOrEmpty()) {
            finishQuietly()
            return
        }
        if (savedInstanceState == null) requestPermissions(wanted, REQUEST)
    }

    override fun onRequestPermissionsResult(
        requestCode: Int,
        permissions: Array<out String>,
        grantResults: IntArray,
    ) {
        super.onRequestPermissionsResult(requestCode, permissions, grantResults)
        finishQuietly()
    }

    /** Ушли, не ответив, — ответ «как есть»: разрешение не выдано. */
    override fun onStop() {
        super.onStop()
        if (!isFinishing) finishQuietly()
    }

    private fun finishQuietly() {
        PermissionBus.answered()
        finish()
        @Suppress("DEPRECATION")
        overridePendingTransition(0, 0)
    }

    companion object {
        const val EXTRA_PERMISSIONS = "app.sufler.PERMISSIONS"
        private const val REQUEST = 1
    }
}

/** Кто ждёт ответа на запрос разрешения. Запрос всегда один. */
object PermissionBus {
    @Volatile
    private var waiting: (() -> Unit)? = null

    @Synchronized
    fun wait(then: () -> Unit) {
        waiting?.invoke()
        waiting = then
    }

    @Synchronized
    fun answered() {
        val then = waiting
        waiting = null
        then?.invoke()
    }
}
