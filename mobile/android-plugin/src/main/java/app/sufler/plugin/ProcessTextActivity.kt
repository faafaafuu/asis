package app.sufler.plugin

import android.app.Activity
import android.content.Intent
import android.os.Bundle

/**
 * Приёмник системного пункта меню «Объяснить» (SPEC §9.4).
 *
 * Android передаёт выделенный текст через [Intent.EXTRA_PROCESS_TEXT]. Активность
 * прозрачная и живёт доли секунды: её задача — забрать текст, положить его в
 * [SelectionBus] и передать управление главной активности приложения, которая уже
 * показывает попап поверх вызывающего приложения.
 */
class ProcessTextActivity : Activity() {

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)

        val text = intent
            ?.getCharSequenceExtra(Intent.EXTRA_PROCESS_TEXT)
            ?.toString()
            .orEmpty()
            .trim()

        if (text.isEmpty()) {
            // Пустое выделение — молча уходим, не мигая окнами.
            finish()
            return
        }

        SelectionBus.publish(text)

        val main = packageManager.getLaunchIntentForPackage(packageName)
        if (main != null) {
            main.addFlags(
                Intent.FLAG_ACTIVITY_NEW_TASK or
                    Intent.FLAG_ACTIVITY_CLEAR_TOP or
                    Intent.FLAG_ACTIVITY_SINGLE_TOP
            )
            main.putExtra(EXTRA_SELECTION, text)
            startActivity(main)
        }

        // Ничего не возвращаем вызывающему приложению: текст мы не меняем,
        // поэтому RESULT_OK с EXTRA_PROCESS_TEXT слал бы обратно правку, которой нет.
        finish()
    }

    companion object {
        const val EXTRA_SELECTION = "app.sufler.SELECTION"
    }
}
