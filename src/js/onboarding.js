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
    hint: "Бесплатный ключ: console.groq.com/keys",
  },
  google: {
    provider: "http",
    endpoint: "https://generativelanguage.googleapis.com/v1beta/openai/chat/completions",
    model: "gemini-2.0-flash",
    key: true,
    hint: "Бесплатный ключ: aistudio.google.com/apikey",
  },
  openrouter: {
    provider: "http",
    endpoint: "https://openrouter.ai/api/v1/chat/completions",
    model: "meta-llama/llama-3.3-70b-instruct:free",
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
