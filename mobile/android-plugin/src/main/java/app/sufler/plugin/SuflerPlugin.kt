package app.sufler.plugin

import android.Manifest
import android.app.Activity
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.ClipboardManager
import android.content.Intent
import android.media.AudioManager
import android.media.ToneGenerator
import android.net.Uri
import android.os.Build
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import android.provider.Settings
import android.speech.RecognitionListener
import android.speech.RecognizerIntent
import android.speech.SpeechRecognizer
import android.speech.tts.TextToSpeech
import android.speech.tts.UtteranceProgressListener
import android.webkit.WebView
import androidx.core.app.NotificationCompat
import androidx.core.app.NotificationManagerCompat
import androidx.core.content.pm.ShortcutInfoCompat
import androidx.core.content.pm.ShortcutManagerCompat
import androidx.core.graphics.drawable.IconCompat
import androidx.core.view.ViewCompat
import androidx.core.view.WindowCompat
import androidx.core.view.WindowInsetsCompat
import app.tauri.PermissionState
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.Permission
import app.tauri.annotation.PermissionCallback
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin
import java.util.Locale
import java.util.UUID
import java.util.concurrent.ConcurrentHashMap

@InvokeArg
class SpeakArgs {
    var text: String = ""
    var rate: Double = 1.0
    var lang: String = "ru-RU"
}

@InvokeArg
class ListenArgs {
    var lang: String = "ru-RU"
}

@InvokeArg
class NotifyArgs {
    var title: String = "Ноа"
    var text: String = ""
}

@InvokeArg
class BarArgs {
    var light: Boolean = false
}

@InvokeArg
class UrlArgs {
    var url: String = ""
}

/**
 * Плагин Tauri для Android — всё, что на телефоне делает система, а не Ноа.
 *
 *   • «Объяснить» в меню выделения: событие `selection` и команда `pendingSelection`;
 *   • голос: `speak` / `stopSpeaking` — системный синтезатор, `listen` /
 *     `stopListening` — системное распознавание, по ходу — событие `speech`
 *     с услышанным и громкостью;
 *   • вызов без рук: жест ассистента, ярлык на иконке и плитка в шторке дают
 *     событие `voiceRequest` (или `pendingVoice`, если приложение запускалось);
 *   • `chime`, `notify`, `openUrl` — сигнал, уведомление, ссылка.
 */
@TauriPlugin(
    permissions = [
        Permission(strings = [Manifest.permission.RECORD_AUDIO], alias = "microphone"),
        Permission(strings = [Manifest.permission.POST_NOTIFICATIONS], alias = "notifications"),
    ]
)
class SuflerPlugin(private val activity: Activity) : Plugin(activity) {

    private val main = Handler(Looper.getMainLooper())

    private var tts: TextToSpeech? = null
    private var ttsReady = false
    private val ttsWaiting = mutableListOf<() -> Unit>()
    /** Незавершённые просьбы сказать — по номеру последнего куска фразы. */
    private val speaking = ConcurrentHashMap<String, Invoke>()

    private var recognizer: SpeechRecognizer? = null
    private var listening: Invoke? = null
    private var lastLevel = 0L

    /** На экране ли приложение: уведомление нужно, только когда его не видно. */
    @Volatile
    private var visible = true

    override fun load(webView: WebView) {
        super.load(webView)
        SelectionBus.listener = { text ->
            val payload = JSObject()
            payload.put("text", text)
            trigger("selection", payload)
        }
        VoiceBus.listener = { trigger("voiceRequest", JSObject()) }
        startTts()
        addShortcut()
        watchInsets(webView)
    }

    /* ── Системные панели ───────────────────────────────────────────────── */

    /** Отступы под строку состояния, жестовую панель и клавиатуру, в CSS-пикселях. */
    @Volatile
    private var insets = JSObject()

    /**
     * С Android 15 приложение рисуется от края до края: страница уходит под
     * часы и под жестовую панель. Отступы страница берёт отсюда — командой
     * `insets` при загрузке и событием `insets`, когда выехала клавиатура.
     */
    private fun watchInsets(webView: WebView) {
        val density = activity.resources.displayMetrics.density
        ViewCompat.setOnApplyWindowInsetsListener(webView) { view, all ->
            val bars = all.getInsets(
                WindowInsetsCompat.Type.systemBars() or WindowInsetsCompat.Type.displayCutout()
            )
            val keyboard = all.getInsets(WindowInsetsCompat.Type.ime())
            val next = JSObject()
            next.put("top", bars.top / density)
            next.put("bottom", maxOf(bars.bottom, keyboard.bottom) / density)
            next.put("left", bars.left / density)
            next.put("right", bars.right / density)
            next.put("keyboard", keyboard.bottom > 0)
            insets = next
            trigger("insets", next)
            ViewCompat.onApplyWindowInsets(view, all)
        }
        ViewCompat.requestApplyInsets(webView)
    }

    @Command
    fun insets(invoke: Invoke) {
        invoke.resolve(insets)
    }

    /** Тёмные значки на светлой странице, светлые на тёмной — под тему Ноа. */
    @Command
    fun barStyle(invoke: Invoke) {
        val light = invoke.parseArgs(BarArgs::class.java).light
        main.post {
            val window = activity.window
            val controller = WindowCompat.getInsetsController(window, window.decorView)
            controller.isAppearanceLightStatusBars = light
            controller.isAppearanceLightNavigationBars = light
            invoke.resolve()
        }
    }

    override fun onPause() {
        visible = false
    }

    override fun onResume() {
        visible = true
    }

    override fun onDestroy() {
        tts?.shutdown()
        recognizer?.destroy()
    }

    /* ── Выделение ──────────────────────────────────────────────────────── */

    /** Забрать текст, пришедший до готовности фронтенда. Пустая строка — текста нет. */
    @Command
    fun pendingSelection(invoke: Invoke) {
        val result = JSObject()
        result.put("text", SelectionBus.takePending().orEmpty())
        invoke.resolve(result)
    }

    /** Просили ли Ноа слушать, пока приложение запускалось. */
    @Command
    fun pendingVoice(invoke: Invoke) {
        val result = JSObject()
        result.put("listen", VoiceBus.takePending())
        invoke.resolve(result)
    }

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

    /* ── Речь ───────────────────────────────────────────────────────────── */

    private fun startTts() {
        tts = TextToSpeech(activity.applicationContext) { status ->
            main.post {
                ttsReady = status == TextToSpeech.SUCCESS
                val queued = ttsWaiting.toList()
                ttsWaiting.clear()
                queued.forEach { it() }
            }
        }
        tts?.setOnUtteranceProgressListener(object : UtteranceProgressListener() {
            override fun onStart(utteranceId: String?) {}

            override fun onDone(utteranceId: String?) {
                utteranceId?.let { speaking.remove(it)?.resolve() }
            }

            override fun onStop(utteranceId: String?, interrupted: Boolean) {
                utteranceId?.let { speaking.remove(it)?.resolve() }
            }

            @Deprecated("Deprecated in Java")
            override fun onError(utteranceId: String?) {
                utteranceId?.let { speaking.remove(it)?.reject("синтезатор речи не смог прочитать") }
            }
        })
    }

    private fun withTts(invoke: Invoke, action: (TextToSpeech) -> Unit) {
        main.post {
            val engine = tts
            when {
                engine != null && ttsReady -> action(engine)
                engine != null -> ttsWaiting.add {
                    if (ttsReady) action(engine)
                    else invoke.reject("На телефоне нет синтезатора речи — поставьте «Синтезатор речи Google».")
                }
                else -> invoke.reject("Синтезатор речи не запущен.")
            }
        }
    }

    /** Прочитать вслух. Ответ приходит, когда фраза дочитана или прервана. */
    @Command
    fun speak(invoke: Invoke) {
        val args = invoke.parseArgs(SpeakArgs::class.java)
        val text = args.text.trim()
        if (text.isEmpty()) {
            invoke.resolve()
            return
        }
        withTts(invoke) { engine ->
            engine.language = Locale.forLanguageTag(args.lang)
            engine.setSpeechRate(args.rate.toFloat().coerceIn(0.5f, 2.0f))
            // Длинный ответ режется по предложениям: у синтезатора предел на
            // одну фразу, и длиннее он молча не читает ничего.
            val parts = split(text, TextToSpeech.getMaxSpeechInputLength() - 100)
            var last = ""
            parts.forEachIndexed { index, part ->
                val id = UUID.randomUUID().toString()
                last = id
                val mode = if (index == 0) TextToSpeech.QUEUE_FLUSH else TextToSpeech.QUEUE_ADD
                engine.speak(part, mode, Bundle(), id)
            }
            speaking[last] = invoke
        }
    }

    /** Замолчать. Недочитанные просьбы считаются исполненными. */
    @Command
    fun stopSpeaking(invoke: Invoke) {
        main.post {
            tts?.stop()
            speaking.values.forEach { it.resolve() }
            speaking.clear()
            invoke.resolve()
        }
    }

    /* ── Распознавание ──────────────────────────────────────────────────── */

    /** Слушать одну фразу. Ответ — `{ text }`; пустой текст — ничего не сказали. */
    @Command
    fun listen(invoke: Invoke) {
        if (getPermissionState("microphone") != PermissionState.GRANTED) {
            requestPermissionForAlias("microphone", invoke, "microphoneAnswered")
            return
        }
        startListening(invoke)
    }

    @PermissionCallback
    private fun microphoneAnswered(invoke: Invoke) {
        if (getPermissionState("microphone") == PermissionState.GRANTED) {
            startListening(invoke)
        } else {
            invoke.reject("Нет доступа к микрофону — разрешите его Ноа в настройках телефона.")
        }
    }

    private fun startListening(invoke: Invoke) {
        val lang = invoke.parseArgs(ListenArgs::class.java).lang
        main.post {
            if (!SpeechRecognizer.isRecognitionAvailable(activity)) {
                invoke.reject(
                    "На телефоне нет службы распознавания речи — поставьте приложение Google."
                )
                return@post
            }
            // Ноа не слушает сам себя: речь прерывается, прежняя запись — тоже.
            tts?.stop()
            listening?.let { finish(it, "") }
            recognizer?.destroy()
            listening = invoke

            val engine = SpeechRecognizer.createSpeechRecognizer(activity)
            recognizer = engine
            engine.setRecognitionListener(object : RecognitionListener {
                override fun onReadyForSpeech(params: Bundle?) = event("ready", "", 0f)
                override fun onBeginningOfSpeech() = event("speaking", "", 0f)
                override fun onRmsChanged(rmsdB: Float) {
                    val now = System.currentTimeMillis()
                    if (now - lastLevel > 80) {
                        lastLevel = now
                        event("level", "", ((rmsdB + 2f) / 12f).coerceIn(0f, 1f))
                    }
                }
                override fun onBufferReceived(buffer: ByteArray?) {}
                override fun onEndOfSpeech() = event("thinking", "", 0f)
                override fun onPartialResults(partialResults: Bundle?) {
                    val text = partialResults
                        ?.getStringArrayList(SpeechRecognizer.RESULTS_RECOGNITION)
                        ?.firstOrNull()
                        .orEmpty()
                    if (text.isNotBlank()) event("partial", text, 0f)
                }
                override fun onEvent(eventType: Int, params: Bundle?) {}

                override fun onResults(results: Bundle?) {
                    val text = results
                        ?.getStringArrayList(SpeechRecognizer.RESULTS_RECOGNITION)
                        ?.firstOrNull()
                        .orEmpty()
                    listening?.let { finish(it, text) }
                }

                override fun onError(error: Int) {
                    val current = listening ?: return
                    when (error) {
                        // Тишина или неразборчиво — не ошибка: сказать было нечего.
                        SpeechRecognizer.ERROR_NO_MATCH,
                        SpeechRecognizer.ERROR_SPEECH_TIMEOUT,
                        SpeechRecognizer.ERROR_CLIENT -> finish(current, "")
                        else -> {
                            listening = null
                            event("idle", "", 0f)
                            current.reject(describe(error))
                        }
                    }
                }
            })

            val intent = Intent(RecognizerIntent.ACTION_RECOGNIZE_SPEECH).apply {
                putExtra(RecognizerIntent.EXTRA_LANGUAGE_MODEL, RecognizerIntent.LANGUAGE_MODEL_FREE_FORM)
                putExtra(RecognizerIntent.EXTRA_LANGUAGE, lang)
                putExtra(RecognizerIntent.EXTRA_PARTIAL_RESULTS, true)
                putExtra(RecognizerIntent.EXTRA_MAX_RESULTS, 1)
                putExtra(RecognizerIntent.EXTRA_CALLING_PACKAGE, activity.packageName)
            }
            engine.startListening(intent)
        }
    }

    private fun finish(invoke: Invoke, text: String) {
        listening = null
        event("idle", "", 0f)
        val result = JSObject()
        result.put("text", text)
        invoke.resolve(result)
    }

    private fun event(state: String, text: String, level: Float) {
        val payload = JSObject()
        payload.put("state", state)
        payload.put("text", text)
        payload.put("level", level.toDouble())
        trigger("speech", payload)
    }

    /** Дослушать сейчас: услышанное уйдёт ответом на `listen`. */
    @Command
    fun stopListening(invoke: Invoke) {
        main.post {
            recognizer?.stopListening()
            invoke.resolve()
        }
    }

    /** Бросить запись без ответа — кнопка «Стоп». */
    @Command
    fun cancelListening(invoke: Invoke) {
        main.post {
            recognizer?.cancel()
            listening?.let { finish(it, "") }
            invoke.resolve()
        }
    }

    private fun describe(error: Int): String = when (error) {
        SpeechRecognizer.ERROR_AUDIO -> "Микрофон не отдал звук."
        SpeechRecognizer.ERROR_INSUFFICIENT_PERMISSIONS ->
            "Нет доступа к микрофону — разрешите его Ноа в настройках телефона."
        SpeechRecognizer.ERROR_NETWORK, SpeechRecognizer.ERROR_NETWORK_TIMEOUT ->
            "Распознаванию речи нужна сеть — проверьте интернет."
        SpeechRecognizer.ERROR_RECOGNIZER_BUSY -> "Распознавание занято — попробуйте ещё раз."
        SpeechRecognizer.ERROR_SERVER -> "Служба распознавания не ответила."
        else -> "Распознать речь не вышло (код $error)."
    }

    /* ── Прочее ─────────────────────────────────────────────────────────── */

    /** Короткий сигнал — будильник, таймер. */
    @Command
    fun chime(invoke: Invoke) {
        try {
            val tone = ToneGenerator(AudioManager.STREAM_NOTIFICATION, 90)
            tone.startTone(ToneGenerator.TONE_PROP_BEEP2, 300)
            main.postDelayed({ tone.release() }, 600)
        } catch (_: RuntimeException) {
            // Звук занят другим приложением — сигнал не критичен.
        }
        invoke.resolve()
    }

    /** Разрешение на уведомления — спрашивается с экрана, не из фона. */
    @Command
    fun allowNotifications(invoke: Invoke) {
        if (Build.VERSION.SDK_INT < 33 ||
            getPermissionState("notifications") == PermissionState.GRANTED
        ) {
            invoke.resolve()
            return
        }
        requestPermissionForAlias("notifications", invoke, "notificationsAnswered")
    }

    @PermissionCallback
    private fun notificationsAnswered(invoke: Invoke) {
        invoke.resolve()
    }

    /** Уведомление в шторке — только когда приложения не видно. */
    @Command
    fun notify(invoke: Invoke) {
        val args = invoke.parseArgs(NotifyArgs::class.java)
        if (visible || args.text.isBlank()) {
            invoke.resolve()
            return
        }
        val manager = NotificationManagerCompat.from(activity)
        if (!manager.areNotificationsEnabled()) {
            invoke.resolve()
            return
        }
        if (Build.VERSION.SDK_INT >= 26) {
            val channel = NotificationChannel(CHANNEL, "Ноа", NotificationManager.IMPORTANCE_HIGH)
            channel.description = "Напоминания, будильники и ответы Ноа"
            activity.getSystemService(NotificationManager::class.java).createNotificationChannel(channel)
        }
        val open = activity.packageManager.getLaunchIntentForPackage(activity.packageName)
        val tap = PendingIntent.getActivity(
            activity, 0, open,
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE
        )
        val notification = NotificationCompat.Builder(activity, CHANNEL)
            .setSmallIcon(activity.applicationInfo.icon)
            .setContentTitle(args.title)
            .setContentText(args.text)
            .setStyle(NotificationCompat.BigTextStyle().bigText(args.text))
            .setPriority(NotificationCompat.PRIORITY_HIGH)
            .setAutoCancel(true)
            .setContentIntent(tap)
            .build()
        try {
            manager.notify((System.currentTimeMillis() % Int.MAX_VALUE).toInt(), notification)
        } catch (_: SecurityException) {
            // Разрешение отозвали между проверкой и показом.
        }
        invoke.resolve()
    }

    /** Открыть ссылку тем, чем человек обычно открывает ссылки. */
    @Command
    fun openUrl(invoke: Invoke) {
        val url = invoke.parseArgs(UrlArgs::class.java).url
        try {
            val intent = Intent(Intent.ACTION_VIEW, Uri.parse(url))
            intent.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
            activity.startActivity(intent)
            invoke.resolve()
        } catch (err: Exception) {
            invoke.reject("Нечем открыть ссылку: ${err.message}")
        }
    }

    /** Текст из буфера обмена — «Вот, показываю» на телефоне. */
    @Command
    fun clipboard(invoke: Invoke) {
        main.post {
            val manager = activity.getSystemService(ClipboardManager::class.java)
            val text = manager?.primaryClip
                ?.takeIf { it.itemCount > 0 }
                ?.getItemAt(0)
                ?.coerceToText(activity)
                ?.toString()
                .orEmpty()
            val result = JSObject()
            result.put("text", text)
            invoke.resolve(result)
        }
    }

    /**
     * Системный выбор помощника: там Ноа назначают на жест ассистента —
     * долгое нажатие «Домой» или кнопки питания.
     */
    @Command
    fun openAssistantSettings(invoke: Invoke) {
        val candidates = listOf(
            Intent("android.settings.MANAGE_DEFAULT_APPS_SETTINGS"),
            Intent(Settings.ACTION_VOICE_INPUT_SETTINGS),
            Intent(Settings.ACTION_SETTINGS),
        )
        for (intent in candidates) {
            try {
                intent.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
                activity.startActivity(intent)
                invoke.resolve()
                return
            } catch (_: Exception) {
                // Такого экрана у этой прошивки нет — пробуем следующий.
            }
        }
        invoke.reject("Не нашлось экрана настроек помощника.")
    }

    /** Ярлык «Спросить голосом» — долгое нажатие на иконку Ноа. */
    private fun addShortcut() {
        try {
            val intent = Intent(activity, AssistActivity::class.java).setAction(Intent.ACTION_ASSIST)
            val shortcut = ShortcutInfoCompat.Builder(activity, "listen")
                .setShortLabel("Спросить голосом")
                .setLongLabel("Спросить Ноа голосом")
                .setIcon(IconCompat.createWithResource(activity, activity.applicationInfo.icon))
                .setIntent(intent)
                .build()
            ShortcutManagerCompat.pushDynamicShortcut(activity, shortcut)
        } catch (_: Exception) {
            // Лаунчер без ярлыков — не беда, остаются кнопка и плитка.
        }
    }

    companion object {
        private const val CHANNEL = "noa"

        /** Режет текст по предложениям так, чтобы кусок не превышал `limit`. */
        fun split(text: String, limit: Int): List<String> {
            if (text.length <= limit) return listOf(text)
            val parts = mutableListOf<String>()
            val current = StringBuilder()
            for (sentence in text.split(Regex("(?<=[.!?…\\n])\\s+"))) {
                if (current.length + sentence.length + 1 > limit && current.isNotEmpty()) {
                    parts.add(current.toString())
                    current.clear()
                }
                if (sentence.length > limit) {
                    sentence.chunked(limit).forEach { parts.add(it) }
                } else {
                    if (current.isNotEmpty()) current.append(' ')
                    current.append(sentence)
                }
            }
            if (current.isNotEmpty()) parts.add(current.toString())
            return parts
        }
    }
}
