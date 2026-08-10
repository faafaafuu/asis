// Окно «доступ и настройка»: показывает, чего именно не хватает системной интеграции.

import { tauri, applyTheme } from "./bridge.js";

const api = tauri();
const ui = {};
for (const node of document.querySelectorAll("[data-el]")) ui[node.dataset.el] = node;

applyTheme("system");

const READY = {
  title: "Всё готово",
  hint: "Выделите текст в любом приложении, удерживая левый Ctrl — рядом появится объяснение.",
};

async function refresh() {
  if (!api) {
    render({
      title: "Режим без системной интеграции",
      hint: "Окно открыто вне приложения, поэтому статус разрешений недоступен.",
      canOpenSettings: false,
    });
    return;
  }

  try {
    const status = await api.invoke("integration_status");
    if (status.kind === "ready") {
      render({ ...READY, canOpenSettings: false });
      return;
    }
    render({
      title: status.title,
      hint: status.hint,
      // Кнопку показываем только там, где системе действительно есть что открыть.
      canOpenSettings: status.kind === "needsPermission",
    });
  } catch (err) {
    render({
      title: "Не удалось проверить доступ",
      hint: String(err),
      canOpenSettings: false,
    });
  }
}

function render({ title, hint, canOpenSettings }) {
  ui.title.textContent = title;
  ui.hint.textContent = hint;
  ui.settings.hidden = !canOpenSettings;
}

/* ── Проверка перехвата ─────────────────────────────────────────────────── */

/**
 * Превращает счётчики наблюдателя в одну строку для пользователя.
 *
 * Что приходит в diag:
 *   gestures — сколько раз поймали жест «отпустил мышь с зажатым левым Ctrl»;
 *   captured — сколько раз из них удалось достать текст;
 *   last     — что случилось в последний раз, уже человеческим языком;
 *   source   — «UI Automation», «буфер обмена» или «—».
 *
 * Возвращает { text, state }, где state: "idle" | "ok" | "warn" —
 * по нему строка красится в нейтральный, акцентный или тревожный цвет.
 */
function verdictFor(diag) {
  const { gestures = 0, captured = 0, last = "", source = "" } = diag ?? {};

  if (gestures === 0) {
    return {
      text:
        "Приложение работает и ждёт жеста.\n" +
        "Выделите слово мышью, удерживая ЛЕВЫЙ Ctrl — правый попап не открывает.",
      state: "idle",
    };
  }

  if (captured === 0) {
    return {
      text:
        `Жест доходит (раз: ${gestures}), но текст получить не удалось ни разу.\n` +
        "Скорее всего программа не показывает выделение системе — включите галочку ниже.\n" +
        `Последнее: ${last}`,
      state: "warn",
    };
  }

  // Захваты есть, но последняя попытка провалилась. Тревожным это не считаем:
  // среди программ всегда найдётся одна упрямая, и красить строку в цвет ошибки
  // из-за неё — значит приучить пользователя не верить этой строке вообще.
  const lastFailed = source === "—";
  return {
    text:
      `Перехват работает: ${captured} из ${gestures}.` +
      (lastFailed ? "" : ` Источник: ${source}.`) +
      `\nПоследнее: ${last}`,
    state: lastFailed ? "idle" : "ok",
  };
}

async function refreshCapture() {
  if (!api) return;
  try {
    const diag = await api.invoke("capture_diagnostics");
    const { text, state } = verdictFor(diag) ?? { text: "", state: "idle" };
    ui.capture.textContent = text;
    ui.capture.dataset.state = state;
    // Диагностика нужна ровно тогда, когда перехват не сработал, — тогда и
    // показываем её сами. Закрывать раздел обратно не станем: человек его уже
    // увидел и вправе решать сам, когда свернуть.
    if (state === "warn") ui.advanced.open = true;
  } catch {
    // Молча: команда опрашивается раз в секунду, и сыпать ошибками в окно,
    // пока пользователь читает соседний раздел, — только мешать.
  }
}

// Опрос, а не событие: наблюдатель живёт в своём потоке и ничего не знает об окне,
// а раз в секунду — достаточно часто, чтобы строка казалась живой.
setInterval(refreshCapture, 1000);

ui.clipboardFallback.addEventListener("change", async () => {
  if (!api) return;
  try {
    const settings = await api.invoke("trigger_settings");
    settings.clipboardFallback = ui.clipboardFallback.checked;
    await api.invoke("save_trigger_settings", { settings });
  } catch (err) {
    ui.capture.textContent = `Не удалось сохранить: ${err}`;
    ui.capture.dataset.state = "warn";
  }
});

ui.logs.addEventListener("click", async () => {
  try {
    await api?.invoke("open_logs");
  } catch (err) {
    ui.capture.textContent = `Журнал не открылся: ${err}`;
    ui.capture.dataset.state = "warn";
  }
});

async function loadTrigger() {
  if (!api) return;
  try {
    const settings = await api.invoke("trigger_settings");
    ui.clipboardFallback.checked = settings.clipboardFallback;
  } catch {
    /* окно открыто вне приложения — настроек нет */
  }
}

/* ── Настройка источника объяснений ─────────────────────────────────────── */

// Готовые наборы: пользователь выбирает сервис, а не заполняет адреса руками.
// Ошибиться в endpoint легко, а понять по «Сбой сети», что дело в опечатке, — нет.
const PRESETS = {
  wikipedia: { provider: "wikipedia", endpoint: "", model: "", key: false },
  groq: {
    provider: "http",
    endpoint: "https://api.groq.com/openai/v1/chat/completions",
    model: "llama-3.3-70b-versatile",
    key: true,
    hint: "Бесплатный ключ: console.groq.com/keys. Из России сервис не отвечает напрямую — включите VPN или впишите прокси ниже.",
  },
  google: {
    provider: "http",
    endpoint: "https://generativelanguage.googleapis.com/v1beta/openai/chat/completions",
    model: "gemini-2.0-flash",
    key: true,
    hint: "Бесплатный ключ: aistudio.google.com/apikey. Из России недоступен — нужен VPN или прокси ниже.",
  },
  openrouter: {
    provider: "http",
    endpoint: "https://openrouter.ai/api/v1/chat/completions",
    model: "openai/gpt-oss-20b:free",
    key: true,
    hint: "Ключ: openrouter.ai/keys — у моделей с пометкой :free платить не нужно",
  },
  ollama: {
    provider: "http",
    endpoint: "http://localhost:11434/api/chat",
    model: "qwen2.5:3b",
    key: false,
    hint: "Модель работает на этом компьютере: ollama pull qwen2.5:3b",
  },
  custom: { provider: "http", endpoint: "", model: "", key: true },
};

/** По сохранённым настройкам понимаем, какой пресет показать. */
function presetFor(settings) {
  if (settings.provider !== "http") return settings.provider === "mock" ? "custom" : "wikipedia";
  const found = Object.entries(PRESETS).find(([, p]) => p.endpoint && p.endpoint === settings.endpoint);
  return found ? found[0] : "custom";
}

function applyPreset(name, { keepValues = false } = {}) {
  const preset = PRESETS[name];
  ui.modelFields.hidden = preset.provider !== "http";
  // Адрес и прокси живут под спойлером, но прятать их надо по тому же признаку:
  // у Википедии нет ни того, ни другого, и пустые поля там только сбивают с толку.
  ui.advancedModel.hidden = preset.provider !== "http";
  ui.keyField.hidden = !preset.key;
  ui.keyHint.textContent = preset.hint ?? "";
  if (!keepValues) {
    ui.endpoint.value = preset.endpoint;
    ui.model.value = preset.model;
  }
}

async function loadSettings() {
  if (!api) return;
  try {
    const settings = await api.invoke("ai_settings");
    const name = presetFor(settings);
    ui.preset.value = name;
    applyPreset(name, { keepValues: true });
    ui.endpoint.value = settings.endpoint || PRESETS[name].endpoint;
    ui.model.value = settings.model || PRESETS[name].model;
    ui.proxy.value = settings.proxy || "";
    ui.apiKey.value = "";
    ui.apiKey.placeholder = settings.apiKey ? "ключ сохранён — оставьте пустым" : "вставьте ключ";
  } catch (err) {
    ui.aiStatus.textContent = `Не удалось прочитать настройки: ${err}`;
  }
}

ui.preset.addEventListener("change", () => applyPreset(ui.preset.value));

ui.save.addEventListener("click", async () => {
  const preset = PRESETS[ui.preset.value];
  ui.aiStatus.textContent = "Сохраняю…";
  try {
    await api.invoke("save_ai_settings", {
      settings: {
        provider: preset.provider,
        endpoint: ui.endpoint.value.trim(),
        apiKey: ui.apiKey.value.trim(),
        model: ui.model.value.trim(),
        proxy: ui.proxy.value.trim(),
      },
    });
    ui.apiKey.value = "";
    ui.aiStatus.textContent = "Сохранено. Нажмите «Проверить», чтобы убедиться, что работает.";
  } catch (err) {
    ui.aiStatus.textContent = `Не удалось сохранить: ${err}`;
  }
});

ui.test.addEventListener("click", async () => {
  ui.aiStatus.textContent = "Проверяю…";
  try {
    // Пробуем настоящий запрос на слове «альбедо»: увидеть ответ надёжнее,
    // чем увидеть «ключ принят».
    const answer = await api.invoke("test_ai");
    ui.aiStatus.textContent = `Работает. Пример ответа: ${answer}`;
  } catch (err) {
    ui.aiStatus.textContent = `Не получилось: ${err}`;
  }
});

ui.recheck.addEventListener("click", refresh);
ui.settings.addEventListener("click", async () => {
  const opened = await api?.invoke("open_permission_settings");
  if (!opened) {
    ui.hint.textContent =
      "Не удалось открыть настройки автоматически — откройте их вручную по инструкции выше.";
  }
});

refresh();
loadSettings();
loadTrigger();
refreshCapture();
