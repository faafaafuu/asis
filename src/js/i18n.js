// Перевод интерфейса. Два языка: русский и английский.
//
// Почему словарь, а не текст прямо в разметке: строки живут и в HTML, и в JS
// (состояния списка моделей, ошибки, подсказки), и без единого места они
// разъезжаются — часть окна переводится, часть остаётся на прежнем языке.
//
// Ключи именованы по смыслу, а не по месту: одна и та же фраза может встретиться
// дважды, а «текст из третьего блока сверху» перестаёт быть правдой при первой
// же перестановке.
//
// Как добавить язык: скопировать блок `en`, перевести значения, добавить код
// в LANGUAGES. Никакой другой код трогать не нужно.

/** Языки, между которыми переключается интерфейс. Порядок — как в меню. */
export const LANGUAGES = [
  { code: "ru", label: "Русский" },
  { code: "en", label: "English" },
];

const STRINGS = {
  ru: {
    "app.name": "Суфлёр",

    "source.title": "Откуда брать объяснения",
    "source.lead": "Википедия работает без ключа. Модель нужна для «простыми словами», примеров и вопросов.",
    "source.label": "Источник",
    "source.wikipedia": "Википедия — без ключа, только определения",
    "source.groq": "Groq — бесплатный ключ, быстрые ответы",
    "source.google": "Google AI Studio — бесплатный ключ",
    "source.openrouter": "OpenRouter — бесплатные модели",
    "source.ollama": "Ollama — модель на этом устройстве",
    "source.custom": "Другой сервис с OpenAI-совместимым API",

    "key.label": "Ключ",
    "key.placeholder": "вставьте ключ",
    "key.saved": "ключ сохранён — оставьте пустым",

    "hint.groq": "Бесплатный ключ: console.groq.com/keys. Из России сервис не отвечает напрямую — включите VPN или впишите прокси ниже.",
    "hint.google": "Бесплатный ключ: aistudio.google.com/apikey. Из России недоступен — нужен VPN или прокси ниже.",
    "hint.openrouter": "Ключ: openrouter.ai/keys — у моделей с пометкой :free платить не нужно",
    "hint.ollama": "Модель работает на этом устройстве, без интернета и ключей",

    "model.label": "Модель",
    "model.compare": "чем отличаются",
    "model.compareHide": "скрыть пояснения",
    "model.showMore": "Показать другие модели",
    "model.showLess": "Свернуть список",
    "model.other": "или имя модели с ollama.com/library",
    "model.download": "Скачать",
    "model.downloaded": "скачана — выбрать",
    "model.chosen": "✓ выбрана",
    "model.absent": "не скачана",
    "model.preparing": "готовлюсь…",

    "models.hint": "Модель скачивается один раз и дальше работает без интернета. Объяснения берутся из выбранной.",
    "models.hintEmpty": "Ни одной модели пока нет. Скачайте любую — это разовое действие, дальше она работает без интернета.",
    "models.hintPulling": "Загрузка идёт в фоне — окно можно закрыть, она не прервётся.",
    "models.ollamaStopped": "Ollama установлена, но не запущена — скачанные модели не видны, пока она молчит.",
    "models.ollamaMissing": "Ollama не найдена. Установите её с ollama.com — программа сама увидит.",
    "models.ollamaMobile":
      "На телефоне Ollama не работает — она для компьютера. Укажите адрес компьютера с Ollama в вашей сети: раскройте «Если что-то не работает» и впишите в «Адрес API» что-то вроде http://192.168.1.5:11434/api/chat",
    "models.ollamaStart": "Запустить Ollama",
    "models.ollamaInstall": "Установить Ollama",
    "models.ollamaInstalling": "Устанавливаю Ollama",
    "models.ollamaInstalled": "Ollama установлена — можно скачивать модели.",
    "models.ollamaInstallFailed": "Не удалось установить Ollama:",
    "models.ollamaStarting": "Запускаю Ollama…",
    "models.ollamaSlow": "Ollama запущена, но пока не отвечает. Подождите немного и нажмите «Обновить».",

    "action.save": "Сохранить",
    "action.test": "Проверить",
    "action.saving": "Сохраняю…",
    "action.saved": "Сохранено. Нажмите «Проверить», чтобы убедиться, что работает.",
    "action.testing": "Проверяю…",
    "action.works": "Работает. Пример ответа:",

    "advanced.title": "Если что-то не работает",
    "capture.title": "Проверка перехвата",
    "capture.lead": "Выделите слово с левым Ctrl — строка ниже обновится сама.",
    "capture.waiting": "Жду выделения…",
    "capture.clipboard": "Забирать выделение копированием, если программа не отдаёт его системе",
    "capture.clipboardHint": "Нужно для Chrome, Telegram, VS Code: они не отдают выделение системе.",
    "capture.idle": "Приложение работает и ждёт жеста.\nВыделите слово мышью, удерживая ЛЕВЫЙ Ctrl — правый попап не открывает.",
    "capture.noText": "Жест доходит, но текст получить не удалось ни разу — включите галочку ниже.",
    "capture.works": "Перехват работает:",
    "capture.of": "из",
    "capture.source": "Источник:",
    "capture.last": "Последнее:",

    "endpoint.label": "Адрес API",
    "proxy.label": "Прокси",
    "proxy.placeholder": "socks5://127.0.0.1:1080 — если сервис недоступен",
    "proxy.hint": "Если ключ есть, а запросы не доходят. Пусто — напрямую.",
    "logs.open": "Открыть журнал",
    "access.recheck": "Проверить доступ",
    "access.openSettings": "Открыть настройки системы",

    "view.title": "Вид",
    "view.theme": "Тема",
    "view.language": "Язык",
    "theme.system": "Как в системе",
    "theme.light": "Светлая",
    "theme.dark": "Тёмная",
    "theme.neon": "Неон",
    "theme.synthwave": "Синтвейв",

    "note.desktop": "Приложение остаётся в трее. Выделение текста с зажатым левым Ctrl открывает объяснение рядом с выделенным словом.",
    "note.mobile": "Выделите текст в любом приложении и выберите «Объяснить» в меню рядом с «Копировать».",

    "popup.analyzing": "Анализирую…",
    "popup.simple": "Простыми словами",
    "popup.examples": "Примеры",
    "popup.thinking": "Думаю…",
    "popup.ask": "Спросить ещё…",
    "popup.expandTitle": "Проще и с примерами",
    "popup.noAnswer": "Ответ не пришёл. Откройте настройку через значок в трее и нажмите «Проверить».",
    "popup.elaborate": "Объясни это простыми словами и приведи один короткий пример.",
  },

  en: {
    "app.name": "Sufler",

    "source.title": "Where explanations come from",
    "source.lead": "Wikipedia works without a key. A model is needed for plain words, examples and questions.",
    "source.label": "Source",
    "source.wikipedia": "Wikipedia — no key, definitions only",
    "source.groq": "Groq — free key, fast answers",
    "source.google": "Google AI Studio — free key",
    "source.openrouter": "OpenRouter — free models",
    "source.ollama": "Ollama — a model on this device",
    "source.custom": "Another service with an OpenAI-compatible API",

    "key.label": "Key",
    "key.placeholder": "paste the key",
    "key.saved": "key saved — leave empty",

    "hint.groq": "Free key: console.groq.com/keys. Blocked in some countries — use a VPN or set a proxy below.",
    "hint.google": "Free key: aistudio.google.com/apikey. Blocked in some countries — use a VPN or set a proxy below.",
    "hint.openrouter": "Key: openrouter.ai/keys — models marked :free cost nothing",
    "hint.ollama": "The model runs on this device, without the internet and without keys",

    "model.label": "Model",
    "model.compare": "how they differ",
    "model.compareHide": "hide notes",
    "model.showMore": "Show other models",
    "model.showLess": "Collapse the list",
    "model.other": "or a model name from ollama.com/library",
    "model.download": "Download",
    "model.downloaded": "downloaded — select",
    "model.chosen": "✓ selected",
    "model.absent": "not downloaded",
    "model.preparing": "preparing…",

    "models.hint": "A model is downloaded once and then works without the internet. Explanations come from the selected one.",
    "models.hintEmpty": "No models yet. Download any — it is a one-time action, afterwards it works offline.",
    "models.hintPulling": "The download runs in the background — you can close this window, it will not stop.",
    "models.ollamaStopped": "Ollama is installed but not running — downloaded models stay invisible while it is silent.",
    "models.ollamaMissing": "Ollama not found. Install it from ollama.com — the app will pick it up.",
    "models.ollamaMobile":
      "Ollama does not run on phones — it is for computers. Point this app at a computer with Ollama on your network: open “If something is not working” and put something like http://192.168.1.5:11434/api/chat into “API address”",
    "models.ollamaStart": "Start Ollama",
    "models.ollamaInstall": "Install Ollama",
    "models.ollamaInstalling": "Installing Ollama",
    "models.ollamaInstalled": "Ollama installed — you can download models now.",
    "models.ollamaInstallFailed": "Could not install Ollama:",
    "models.ollamaStarting": "Starting Ollama…",
    "models.ollamaSlow": "Ollama started but is not responding yet. Wait a moment and press “Refresh”.",

    "action.save": "Save",
    "action.test": "Test",
    "action.saving": "Saving…",
    "action.saved": "Saved. Press “Test” to make sure it works.",
    "action.testing": "Testing…",
    "action.works": "Works. Sample answer:",

    "advanced.title": "If something is not working",
    "capture.title": "Selection check",
    "capture.lead": "Select a word holding the left Ctrl — the line below updates itself.",
    "capture.waiting": "Waiting for a selection…",
    "capture.clipboard": "Take the selection by copying if the app does not hand it to the system",
    "capture.clipboardHint": "Needed for Chrome, Telegram, VS Code: they do not hand the selection to the system.",
    "capture.idle": "The app is running and waiting for the gesture.\nSelect a word with the mouse holding the LEFT Ctrl — the right one does not open the popup.",
    "capture.noText": "The gesture arrives, but the text was never captured — tick the box below.",
    "capture.works": "Selection works:",
    "capture.of": "of",
    "capture.source": "Source:",
    "capture.last": "Last:",

    "endpoint.label": "API address",
    "proxy.label": "Proxy",
    "proxy.placeholder": "socks5://127.0.0.1:1080 — if the service is unreachable",
    "proxy.hint": "For when the key is right but requests do not arrive. Empty — go direct.",
    "logs.open": "Open the log",
    "access.recheck": "Check access",
    "access.openSettings": "Open system settings",

    "view.title": "View",
    "view.theme": "Theme",
    "view.language": "Language",
    "theme.system": "Match the system",
    "theme.light": "Light",
    "theme.dark": "Dark",
    "theme.neon": "Neon",
    "theme.synthwave": "Synthwave",

    "note.desktop": "The app stays in the tray. Selecting text with the left Ctrl held opens an explanation next to the word.",
    "note.mobile": "Select text in any app and choose “Explain” in the menu next to “Copy”.",

    "popup.analyzing": "Analysing…",
    "popup.simple": "In plain words",
    "popup.examples": "Examples",
    "popup.thinking": "Thinking…",
    "popup.ask": "Ask more…",
    "popup.expandTitle": "Simpler, with examples",
    "popup.noAnswer": "No answer arrived. Open settings from the tray icon and press “Test”.",
    "popup.elaborate": "Explain this in plain words and give one short example.",
  },
};

/** Текущий язык. Меняется через `setLanguage`, по умолчанию русский. */
let current = "ru";

export function setLanguage(code) {
  current = STRINGS[code] ? code : "ru";
  document.documentElement.lang = current;
}

export function language() {
  return current;
}

/**
 * Строка по ключу.
 *
 * Неизвестный ключ возвращается как есть — это заметно на экране и потому
 * чинится, в отличие от пустоты, которую легко не заметить при беглой проверке.
 */
export function t(key) {
  return STRINGS[current]?.[key] ?? STRINGS.ru[key] ?? key;
}

/**
 * Проставляет переводы в разметку: `data-i18n` — в текст, `data-i18n-ph` —
 * в подсказку поля ввода, `data-i18n-title` — во всплывающую подсказку.
 */
export function translateDom(root = document) {
  for (const node of root.querySelectorAll("[data-i18n]")) {
    node.textContent = t(node.dataset.i18n);
  }
  for (const node of root.querySelectorAll("[data-i18n-ph]")) {
    node.placeholder = t(node.dataset.i18nPh);
  }
  for (const node of root.querySelectorAll("[data-i18n-title]")) {
    node.title = t(node.dataset.i18nTitle);
  }
}
