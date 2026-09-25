package app.sufler.plugin

import android.app.AlarmManager
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.media.AudioAttributes
import android.media.RingtoneManager
import android.os.Build
import androidx.core.app.NotificationCompat
import androidx.core.app.NotificationManagerCompat
import org.json.JSONArray
import org.json.JSONObject

/**
 * Будильники, таймеры и напоминания — через AlarmManager.
 *
 * Пока приложение открыто, о них говорит сам Ноа. Но Android закрывает
 * свёрнутые приложения, когда ему нужна память, и тогда напоминание из
 * памяти программы не прозвучит. Поэтому Ноа отдаёт системе расписание,
 * а в нужную минуту уведомление показывает [ReminderReceiver] — даже если
 * приложения уже нет в памяти. Расписание хранится, чтобы после
 * перезагрузки телефона его можно было поставить заново.
 */
object Schedule {
    private const val PREFS = "noa.schedule"
    private const val KEY = "items"
    const val CHANNEL_REMIND = "noa"
    const val CHANNEL_ALARM = "noa.alarm"

    /** Ставит расписание целиком: старое снимается, новое ставится. */
    fun apply(context: Context, items: JSONArray) {
        val prefs = context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
        val old = JSONArray(prefs.getString(KEY, "[]"))
        val manager = context.getSystemService(AlarmManager::class.java) ?: return
        for (i in 0 until old.length()) {
            manager.cancel(pending(context, old.getJSONObject(i)))
        }
        val now = System.currentTimeMillis()
        for (i in 0 until items.length()) {
            val item = items.getJSONObject(i)
            val at = item.optLong("at")
            if (at <= now) continue
            val intent = pending(context, item)
            val exact = Build.VERSION.SDK_INT < 31 || manager.canScheduleExactAlarms()
            when {
                // Будильник — «настоящий»: значок в строке состояния, точно в срок.
                item.optBoolean("alarm") && exact -> manager.setAlarmClock(
                    AlarmManager.AlarmClockInfo(at, launch(context)),
                    intent,
                )
                exact -> manager.setExactAndAllowWhileIdle(AlarmManager.RTC_WAKEUP, at, intent)
                else -> manager.setAndAllowWhileIdle(AlarmManager.RTC_WAKEUP, at, intent)
            }
        }
        prefs.edit().putString(KEY, items.toString()).apply()
    }

    /** После перезагрузки — то же расписание заново. */
    fun restore(context: Context) {
        val prefs = context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
        apply(context, JSONArray(prefs.getString(KEY, "[]")))
    }

    private fun pending(context: Context, item: JSONObject): PendingIntent {
        val intent = Intent(context, ReminderReceiver::class.java)
            .setAction("app.sufler.REMIND")
            .putExtra("id", item.optString("id"))
            .putExtra("title", item.optString("title"))
            .putExtra("text", item.optString("text"))
            .putExtra("alarm", item.optBoolean("alarm"))
        return PendingIntent.getBroadcast(
            context,
            item.optString("id").hashCode(),
            intent,
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
        )
    }

    fun launch(context: Context): PendingIntent? {
        val open = context.packageManager.getLaunchIntentForPackage(context.packageName) ?: return null
        return PendingIntent.getActivity(
            context, 0, open,
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
        )
    }

    fun channels(context: Context) {
        if (Build.VERSION.SDK_INT < 26) return
        val manager = context.getSystemService(NotificationManager::class.java) ?: return
        val remind = NotificationChannel(CHANNEL_REMIND, "Ноа", NotificationManager.IMPORTANCE_HIGH)
        remind.description = "Напоминания, таймеры и ответы Ноа"
        val alarm = NotificationChannel(CHANNEL_ALARM, "Будильник Ноа", NotificationManager.IMPORTANCE_HIGH)
        alarm.description = "Будильники, поставленные Ноа"
        alarm.setSound(
            RingtoneManager.getDefaultUri(RingtoneManager.TYPE_ALARM),
            AudioAttributes.Builder().setUsage(AudioAttributes.USAGE_ALARM).build(),
        )
        manager.createNotificationChannels(listOf(remind, alarm))
    }

    /** Уведомление в шторке. */
    fun show(context: Context, title: String, text: String, alarm: Boolean) {
        val manager = NotificationManagerCompat.from(context)
        if (!manager.areNotificationsEnabled() || text.isBlank()) return
        channels(context)
        val notification = NotificationCompat.Builder(context, if (alarm) CHANNEL_ALARM else CHANNEL_REMIND)
            .setSmallIcon(context.applicationInfo.icon)
            .setContentTitle(title)
            .setContentText(text)
            .setStyle(NotificationCompat.BigTextStyle().bigText(text))
            .setPriority(NotificationCompat.PRIORITY_HIGH)
            .setCategory(if (alarm) NotificationCompat.CATEGORY_ALARM else NotificationCompat.CATEGORY_REMINDER)
            .setAutoCancel(true)
            .setContentIntent(launch(context))
            .build()
        try {
            manager.notify((System.currentTimeMillis() % Int.MAX_VALUE).toInt(), notification)
        } catch (_: SecurityException) {
            // Разрешение на уведомления отозвали.
        }
    }
}

/**
 * Срок пришёл. Приложение на экране — о нём говорит сам Ноа, уведомление
 * не нужно; свёрнуто или закрыто — уведомление в шторке.
 */
class ReminderReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        when (intent.action) {
            Intent.ACTION_BOOT_COMPLETED, Intent.ACTION_MY_PACKAGE_REPLACED -> Schedule.restore(context)
            "app.sufler.REMIND" -> {
                if (SuflerPlugin.visible) return
                Schedule.show(
                    context,
                    intent.getStringExtra("title") ?: "Ноа",
                    intent.getStringExtra("text").orEmpty(),
                    intent.getBooleanExtra("alarm", false),
                )
            }
        }
    }
}
