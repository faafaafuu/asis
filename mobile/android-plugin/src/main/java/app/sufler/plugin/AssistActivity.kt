package app.sufler.plugin

import android.app.Activity
import android.content.Intent
import android.os.Bundle

/**
 * Вызов Ноа без рук — то, чем на компьютере служат левый Alt с пробелом и
 * имя вслух. Сюда приводят жест ассистента (долгое нажатие «Домой» или
 * кнопки питания, если Ноа выбран помощником), ярлык «Спросить голосом» на
 * иконке и плитка в быстрых настройках. Активность прозрачная: она только
 * будит приложение и просит экран разговора сразу слушать.
 */
class AssistActivity : Activity() {

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        VoiceBus.request()
        packageManager.getLaunchIntentForPackage(packageName)?.let { main ->
            main.addFlags(
                Intent.FLAG_ACTIVITY_NEW_TASK or
                    Intent.FLAG_ACTIVITY_CLEAR_TOP or
                    Intent.FLAG_ACTIVITY_SINGLE_TOP
            )
            startActivity(main)
        }
        finish()
    }
}
