// Окно «доступ и настройка»: показывает, чего именно не хватает системной интеграции.

import { tauri, applyTheme } from "./bridge.js";
import { PopupView } from "./popup-view.js";
import { TauriProvider, DEFAULT_ERROR_TEXT } from "./ai-client.js";
import { attachMobileEntry } from "./mobile-entry.js";
import { LANGUAGES, setLanguage, t, translateDom } from "./i18n.js";

const api = tauri();
const ui = {};
for (const node of document.querySelectorAll("[data-el]")) ui[node.dataset.el] = node;

// Тему и язык Rust проставил до первого кадра — здесь только подхватываем,
// чтобы не сбросить их обратно на значения по умолчанию.
const injected = globalThis.__SUFLER_VIEW__;

/* ── Вид: тема и язык ───────────────────────────────────────────────────── */

/** Темы в порядке меню. Названия переводятся, коды — нет. */
const THEMES = ["system", "light", "dark", "neon", "synthwave"];

let view = { theme: injected?.theme ?? "system", language: injected?.language ?? "ru" };

/**
 * Перерисовывает всё, что зависит от языка.
 *
 * Дважды: сначала статическая разметка по data-i18n, затем живые части —
 * список моделей и строка перехвата собираются кодом и о разметке не знают.
 */
function applyLanguage(code) {
  setLanguage(code);
  translateDom();
  renderViewMenu();
  refreshCapture();
  if (ui.preset) applyPreset(ui.preset.value, { keepValues: true });
  if (!ui.modelsBlock.hidden) redraw();
}

function renderViewMenu() {
  const group = (list, current, onPick) => {
    const box = document.createElement("div");
    for (const item of list) {
      const button = document.createElement("button");
      button.type = "button";
      button.className = "ob__menu-item";
      button.textContent = item.label;
      button.setAttribute("role", "menuitemradio");
      button.setAttribute("aria-checked", String(item.code === current));
      button.addEventListener("click", () => onPick(item.code));
      box.append(button);
    }
    return box.children;
  };

  ui.themeList.replaceChildren(
    ...group(
      THEMES.map((code) => ({ code, label: t(`theme.${code}`) })),
      view.theme,
      (code) => saveView({ ...view, theme: code }),
    ),
  );
  ui.langList.replaceChildren(
    ...group(LANGUAGES, view.language, (code) => saveView({ ...view, language: code })),
  );
}

async function saveView(next) {
  view = next;
  applyTheme(view.theme);
  applyLanguage(view.language);
  try {
    await api?.invoke("save_appearance", { appearance: view });
  } catch (err) {
    ui.aiStatus.textContent = `${err}`;
  }
}

async function loadView() {
  // Язык применяем сразу, из подставленного Rust значения: ждать ответа по IPC
  // значило бы показать секунду русского текста человеку, выбравшему английский.
  applyTheme(view.theme);
  applyLanguage(view.language);
  if (!api) return;

  // Перечитываем на случай, если настройки поменяли в обход этого окна.
  try {
    const fresh = await api.invoke("appearance");
    if (fresh.theme === view.theme && fresh.language === view.language) return;
    view = fresh;
    applyTheme(view.theme);
    applyLanguage(view.language);
  } catch {
    /* окно открыто вне приложения — остаёмся на подставленных значениях */
  }
}

ui.viewBtn.addEventListener("click", (event) => {
  event.stopPropagation();
  const open = ui.viewMenu.hidden;
  ui.viewMenu.hidden = !open;
  ui.viewBtn.setAttribute("aria-expanded", String(open));
});

// Щелчок мимо меню закрывает его: отдельной кнопки «закрыть» у выпадающего
// списка быть не должно, а оставлять его висеть — значит спорить с привычкой.
document.addEventListener("click", (event) => {
  if (ui.viewMenu.hidden || ui.viewMenu.contains(event.target)) return;
  ui.viewMenu.hidden = true;
  ui.viewBtn.setAttribute("aria-expanded", "false");
});

document.addEventListener("keydown", (event) => {
  if (event.key === "Escape" && !ui.viewMenu.hidden) {
    ui.viewMenu.hidden = true;
    ui.viewBtn.setAttribute("aria-expanded", "false");
  }
});

/**
 * Телефон это или компьютер — приходит из Rust, где известно на этапе сборки.
 *
 * Окно настройки одно на все системы, но часть его верна только на компьютере:
 * ни трея, ни левого Ctrl, ни локальной Ollama на телефоне нет.
 */
let isMobile = false;

async function applyPlatform() {
  if (!api) return;
  let config;
  try {
    config = await api.invoke("runtime_config");
    isMobile = Boolean(config.mobile);
  } catch {
    // Не узнали — остаёмся на настольном варианте: он и был всё это время.
    return;
  }
  if (!isMobile) return;

  ui.note.textContent = t("note.mobile");
  // Проверка перехвата — про мышь и левый Ctrl, на телефоне проверять нечего.
  ui.captureBlock.hidden = true;

  attachExplainOverlay(config);
}

/**
 * Показ объяснения поверх настроек — только на телефоне.
 *
 * На компьютере попап живёт отдельным окном, которое Rust ставит рядом с
 * выделенным словом. На телефоне окон не бывает: приложение занимает экран
 * целиком, и второму окну взяться неоткуда. Поэтому текст из пункта «Объяснить»
 * показываем прямо здесь, поверх настроек.
 *
 * Сама связка «нативный плагин → событие → попап» была написана давно и лежала
 * без дела: её подключал popup-window.js, а он на телефоне не открывается —
 * там главным окном идёт эта страница. Отсюда и тишина в ответ на пункт меню.
 */
function attachExplainOverlay(config) {
  const view = new PopupView({
    client: new TauriProvider(api.invoke),
    errorText: config.errorText || DEFAULT_ERROR_TEXT,
    dialogue: Boolean(config.dialogue),
    // Размер окна на телефоне не наш: страница и так во весь экран.
    onGeometry: () => {},
    onClose: () => {
      ui.overlay.hidden = true;
      view.close();
    },
  });
  ui.overlayMount.append(view.el);

  ui.overlayClose.addEventListener("click", () => {
    ui.overlay.hidden = true;
    view.close();
  });

  attachMobileEntry(api, (term) => {
    ui.overlay.hidden = false;
    view.open({ term, context: "" });
  });
}

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
      render({ ...READY, canOpenSettings: false, ready: true });
      return;
    }
    render({
      title: status.title,
      hint: status.hint,
      // Кнопку показываем только там, где системе действительно есть что открыть.
      canOpenSettings: status.kind === "needsPermission",
      ready: false,
    });
  } catch (err) {
    render({
      title: "Не удалось проверить доступ",
      hint: String(err),
      canOpenSettings: false,
      ready: false,
    });
  }
}

function render({ title, hint, canOpenSettings, ready }) {
  ui.title.textContent = title;
  ui.hint.textContent = hint;
  ui.settings.hidden = !canOpenSettings;

  // «Всё готово» — сообщение, которое читают один раз, а место оно занимало
  // всегда, отодвигая вниз единственное, зачем окно открывают: выбор источника.
  // Когда доступ есть, заголовок и кнопка проверки не нужны — они переезжают
  // в раздел «Если что-то не работает», где их и станут искать. Когда доступа
  // нет, всё наоборот: это главное сообщение окна, и оно наверху.
  ui.status.hidden = ready;
  ui.recheckHere.hidden = !ready;
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
    return { text: t("capture.idle"), state: "idle" };
  }

  if (captured === 0) {
    return {
      text: `${t("capture.noText")} (${gestures})\n${t("capture.last")} ${last}`,
      state: "warn",
    };
  }

  // Захваты есть, но последняя попытка провалилась. Тревожным это не считаем:
  // среди программ всегда найдётся одна упрямая, и красить строку в цвет ошибки
  // из-за неё — значит приучить пользователя не верить этой строке вообще.
  const lastFailed = source === "—";
  return {
    text:
      `${t("capture.works")} ${captured} ${t("capture.of")} ${gestures}.` +
      (lastFailed ? "" : ` ${t("capture.source")} ${source}.`) +
      `\n${t("capture.last")} ${last}`,
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
    hintKey: "hint.groq",
  },
  google: {
    provider: "http",
    endpoint: "https://generativelanguage.googleapis.com/v1beta/openai/chat/completions",
    model: "gemini-2.0-flash",
    key: true,
    hintKey: "hint.google",
  },
  openrouter: {
    provider: "http",
    endpoint: "https://openrouter.ai/api/v1/chat/completions",
    model: "openai/gpt-oss-20b:free",
    key: true,
    hintKey: "hint.openrouter",
  },
  ollama: {
    provider: "http",
    endpoint: "http://localhost:11434/api/chat",
    model: "qwen2.5:3b",
    key: false,
    hintKey: "hint.ollama",
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
  ui.keyHint.textContent = preset.hintKey ? t(preset.hintKey) : "";

  // У Ollama модели лежат на этом же компьютере — их можно показать списком
  // и доставить недостающую. У облачных сервисов список бесконечен и меняется
  // без предупреждения, там честнее оставить поле для ввода.
  const local = name === "ollama";
  ui.modelsBlock.hidden = !local;
  ui.modelField.hidden = local;
  if (local) refreshModels();
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
    ui.apiKey.placeholder = settings.apiKey ? t("key.saved") : t("key.placeholder");
  } catch (err) {
    ui.aiStatus.textContent = `Не удалось прочитать настройки: ${err}`;
  }
}

ui.preset.addEventListener("change", () => applyPreset(ui.preset.value));

async function saveAi() {
  const preset = PRESETS[ui.preset.value];
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
}

ui.save.addEventListener("click", async () => {
  ui.aiStatus.textContent = t("action.saving");
  try {
    await saveAi();
    ui.aiStatus.textContent = t("action.saved");
  } catch (err) {
    ui.aiStatus.textContent = `Не удалось сохранить: ${err}`;
  }
});

/* ── Модели на этом устройстве ──────────────────────────────────────────── */

// Что предлагаем поставить. Список короткий намеренно: каждая модель здесь
// проверена на настоящих терминах, а не взята из чужого рейтинга. Первая
// отвечает точнее и знает научные слова, вторая легче и шустрее, но на редких
// терминах ошибается. Всё, что уже установлено, показывается и без этого списка.
// Размеры взяты из реестра Ollama (registry.ollama.ai), а не по памяти:
// у каждой строки это настоящий объём загрузки.
//
// Первые три проверены на живых терминах, и подпись у них про качество.
// Остальные я не проверял — и подпись у них поэтому только о том, что можно
// утверждать наверняка: чьё семейство и насколько крупная. Придумывать им
// достоинства значило бы выдать догадку за рекомендацию.
const CATALOG = [
  { name: "qwen2.5:7b", size: "4.7 ГБ", note: "точнее всех, знает термины", top: true },
  { name: "gemma3:4b", size: "3.3 ГБ", note: "быстрее, на редких словах слабее", top: true },
  { name: "gemma3:1b", size: "0.8 ГБ", note: "для слабых машин, чаще ошибается", top: true },
  { name: "qwen2.5:3b", size: "1.9 ГБ", note: "то же семейство, что и первая, но меньше" },
  { name: "llama3.2:3b", size: "2.0 ГБ", note: "Llama от Meta, компактная" },
  { name: "phi4-mini", size: "2.5 ГБ", note: "Phi от Microsoft, компактная" },
  { name: "mistral:7b", size: "4.4 ГБ", note: "Mistral, размером с первую" },
  { name: "qwen3:4b", size: "2.5 ГБ", note: "новее qwen2.5, но думает перед ответом дольше" },
  { name: "gemma3:12b", size: "8.1 ГБ", note: "самая крупная здесь, нужна мощная машина" },
];

/**
 * Развёрнут ли полный каталог.
 *
 * По умолчанию видно три строки. Девять незнакомых имён на первом экране —
 * это не выбор, а работа по сравнению того, о чём человек ничего не знает.
 */
let showAllModels = false;

/** Показывать ли пояснения к моделям. По умолчанию нет — см. разметку. */
let showModelNotes = false;

/** Идущие сейчас загрузки: имя модели → строка состояния для показа. */
const pulling = new Map();

/**
 * Последний известный список установленного.
 *
 * Прогресс загрузки приходит десятки раз в минуту, и спрашивать у Ollama состав
 * моделей на каждый процент — бессмысленная работа: за время загрузки он не
 * меняется. Перерисовываем по запомненному.
 */
let lastInstalled = [];

/** Отвечала ли Ollama при последнем опросе. */
let lastRunning = false;

/**
 * Последняя неудача при работе с моделями — держится до следующего действия.
 * Отдельно от подсказки: подсказку перерисовка переписывает каждый раз.
 */
let modelsProblem = "";

function makeRow({ name, note, size, installed, chosen }) {
  const row = document.createElement("div");
  row.className = "ob__model";
  if (chosen) row.dataset.chosen = "true";
  // Нескачанная строка приглушена: по одному взгляду на список должно быть
  // видно, что у тебя есть, а что придётся ждать.
  if (!installed) row.dataset.absent = "true";

  // Имя и размер слева одной группой, состояние — справа. Размер показываем
  // всегда, а не только у нескачанных: у скачанной он отвечает на вопрос
  // «сколько это занимает у меня на диске». Это часть решения, не справка.
  const main = document.createElement("span");
  main.className = "ob__model-main";

  const title = document.createElement("span");
  title.className = "ob__model-name";
  title.textContent = name;

  const sizeEl = document.createElement("span");
  sizeEl.className = "ob__model-size";
  sizeEl.textContent = size;

  main.append(title, sizeEl);
  row.append(main);

  // Наведение показывает пояснение и без раскрытия списка: тому, кто просто
  // хочет узнать про одну строку, незачем разворачивать все девять.
  if (note) row.title = note;

  const state = document.createElement("span");
  state.className = "ob__model-state";

  if (pulling.has(name)) {
    state.textContent = pulling.get(name);
    row.append(state);
  } else if (installed) {
    // Выбор — щелчок по строке целиком, а не по крошечной галочке: строк мало,
    // промахнуться не по чему.
    row.tabIndex = 0;
    row.dataset.pick = name;
    // Состояние словами: строка без кнопки выглядела так же, как строка с
    // кнопкой, и понять, что уже лежит на диске, было нельзя.
    state.textContent = chosen ? t("model.chosen") : t("model.downloaded");
    row.append(state);
  } else {
    state.textContent = t("model.absent");
    const button = document.createElement("button");
    button.type = "button";
    button.className = "ob__btn ob__btn--quiet";
    button.dataset.pull = name;
    button.textContent = t("model.download");
    row.append(state, button);
  }

  // Пояснение — отдельной строкой под именем, поэтому список переносит его
  // на второй ряд только когда пояснения включены (см. CSS).
  if (note) {
    const hint = document.createElement("span");
    hint.className = "ob__model-note";
    hint.textContent = note;
    row.append(hint);
  }

  return row;
}

function renderModels(status) {
  ui.modelsList.replaceChildren();
  ui.modelsList.dataset.notes = showModelNotes ? "on" : "off";
  ui.modelsInfo.textContent = showModelNotes ? t("model.compareHide") : t("model.compare");
  lastInstalled = status?.installed ?? lastInstalled;
  lastRunning = status?.running ?? false;

  if (!status?.running) {
    // На телефоне Ollama не бывает вовсе — это программа для компьютера.
    // Советовать «установите её» бессмысленно, а кнопка запуска не запустит
    // ничего. Зато телефон может спрашивать компьютер в той же сети — это
    // единственный рабочий путь, о нём и говорим.
    if (isMobile) {
      ui.ollamaStart.hidden = true;
      ui.modelsHint.textContent = t("models.ollamaMobile");
      return;
    }

    // Установлена, но молчит — обычное дело после перезагрузки: Ollama не
    // всегда прописывается в автозапуск. Советовать «установите с ollama.com»
    // тому, у кого она уже стоит, — значит переложить вину на человека и не
    // сказать, что делать. Предлагаем запустить прямо отсюда.
    // Установлена, но молчит — предлагаем запустить. Не найдена вовсе —
    // предлагаем поставить: ходить за ней на сайт вручную человек не обязан.
    ui.ollamaStart.hidden = !status?.present;
    ui.ollamaInstall.hidden = Boolean(status?.present);
    if (!status?.present) labelInstallButton();
    ui.modelsHint.textContent = status?.present
      ? t("models.ollamaStopped")
      : t("models.ollamaMissing");
    return;
  }

  ui.ollamaStart.hidden = true;
  ui.ollamaInstall.hidden = true;

  const installed = new Map(status.installed.map((m) => [m.name, m]));
  const chosen = ui.model.value.trim();
  const shown = new Set();

  for (const item of showAllModels ? CATALOG : CATALOG.filter((m) => m.top)) {
    shown.add(item.name);
    const found = installed.get(item.name);
    ui.modelsList.append(
      makeRow({
        name: item.name,
        note: item.note,
        // У скачанной берём настоящий размер с диска, у остальной — обещанный
        // из списка: до загрузки точного размера никто не знает.
        size: found ? `${found.sizeGb} ГБ` : item.size,
        installed: Boolean(found),
        chosen: Boolean(found) && chosen === item.name,
      }),
    );
  }

  // Всё остальное, что человек скачал сам, — тоже его выбор, прятать нельзя.
  for (const model of status.installed) {
    if (shown.has(model.name)) continue;
    shown.add(model.name);
    ui.modelsList.append(
      makeRow({
        name: model.name,
        note: "",
        size: `${model.sizeGb} ГБ`,
        installed: true,
        chosen: chosen === model.name,
      }),
    );
  }

  // Модель, которую качают прямо сейчас, может не быть ни в списке, ни среди
  // установленных — так бывает с любой, вписанной руками. Без этой строки
  // загрузка шла бы вслепую: кнопку нажали, а на экране ничего не изменилось.
  for (const name of pulling.keys()) {
    if (shown.has(name)) continue;
    ui.modelsList.append(makeRow({ name, note: "", size: "", installed: false, chosen: false }));
  }

  ui.modelsToggle.textContent = showAllModels ? t("model.showLess") : t("model.showMore");
  ui.modelAdd.hidden = !showAllModels;

  // Сообщение о неудаче переживает перерисовку и держится, пока человек не
  // начнёт новое действие. Раньше его писали прямо в подсказку, а следующая
  // же строка этой функции затирала текст дежурным пояснением — сообщение
  // жило миллисекунды, и со стороны нажатие «Скачать» выглядело так, будто
  // кнопка ничего не делает. Именно так и терялась ошибка «Ollama не отвечает».
  if (modelsProblem) {
    ui.modelsHint.textContent = modelsProblem;
  } else if (pulling.size) {
    ui.modelsHint.textContent = t("models.hintPulling");
  } else if (installed.size === 0) {
    ui.modelsHint.textContent = t("models.hintEmpty");
  } else {
    ui.modelsHint.textContent = t("models.hint");
  }
}

async function refreshModels() {
  if (!api) return;
  try {
    renderModels(await api.invoke("local_models"));
  } catch {
    renderModels(null);
  }
}

/**
 * Качает модель по имени — из списка или вписанную руками, разницы нет.
 *
 * Строку состояния ставим до запроса, а не по первому событию: между нажатием
 * и первым ответом Ollama проходит секунда-другая, и всё это время окно
 * выглядело бы так, будто кнопку не нажали.
 */
async function startPull(name) {
  if (!name || pulling.has(name)) return;
  modelsProblem = "";
  pulling.set(name, t("model.preparing"));
  redraw();

  try {
    await api.invoke("pull_model", { model: name });
  } catch (err) {
    // Сюда попадает и опечатка в имени, и молчащая Ollama: сообщение держится
    // до следующего действия, а не гаснет на ближайшей перерисовке.
    modelsProblem = `Не удалось скачать «${name}»: ${err}`;
    pulling.delete(name);
    // Заодно уточняем, жива ли Ollama вообще: если нет — покажется кнопка
    // «Запустить», а не одно лишь сообщение о неудаче.
    await refreshModels();
    return;
  }

  pulling.delete(name);
  await refreshModels();
}

/**
 * Перерисовка по запомненному состоянию — без похода в Rust.
 *
 * Раньше в таких местах писалось `{ running: true }`, то есть окно уверяло
 * само себя, что Ollama отвечает, даже когда она молчала.
 */
function redraw() {
  renderModels({ running: lastRunning, present: true, installed: lastInstalled });
}

ui.modelsToggle.addEventListener("click", () => {
  showAllModels = !showAllModels;
  redraw();
});

ui.modelsInfo.addEventListener("click", () => {
  showModelNotes = !showModelNotes;
  redraw();
});

/**
 * Размер установщика Ollama — пишем прямо на кнопке.
 *
 * Полтора гигабайта человек должен видеть до нажатия, а не узнавать из
 * ползущей полосы. Спрашиваем у GitHub один раз, при первом показе кнопки;
 * не ответил — кнопка остаётся без размера, но работает.
 */
let installSize = null;

async function labelInstallButton() {
  if (installSize === null && api) {
    try {
      installSize = await api.invoke("ollama_install_size");
    } catch {
      installSize = 0;
    }
  }
  ui.ollamaInstall.textContent = installSize
    ? `${t("models.ollamaInstall")} · ${installSize} ГБ`
    : t("models.ollamaInstall");
}

ui.ollamaInstall.addEventListener("click", async () => {
  modelsProblem = "";
  ui.ollamaInstall.disabled = true;
  try {
    await api.invoke("install_ollama");
    modelsProblem = t("models.ollamaInstalled");
  } catch (err) {
    modelsProblem = `${t("models.ollamaInstallFailed")} ${err}`;
  }
  ui.ollamaInstall.disabled = false;
  await refreshModels();
});

// Ход установки: те же события, что у загрузки моделей, но своим каналом —
// путать «качается модель» и «ставится сама Ollama» нельзя.
api?.listen("ollama:install", (event) => {
  const { percent, status, done } = event.payload ?? {};
  if (done) return;
  ui.ollamaInstall.textContent =
    percent > 0 ? `${t("models.ollamaInstalling")} · ${status} ${percent}%` : `${t("models.ollamaInstalling")} · ${status}`;
});

ui.ollamaStart.addEventListener("click", async () => {
  modelsProblem = "";
  ui.ollamaStart.disabled = true;
  ui.modelsHint.textContent = t("models.ollamaStarting");
  try {
    await api.invoke("start_ollama");
  } catch (err) {
    modelsProblem = `Не удалось запустить: ${err}`;
    ui.ollamaStart.disabled = false;
    await refreshModels();
    return;
  }

  // Сервер поднимается несколько секунд. Спрашиваем раз в секунду, пока не
  // ответит: молчаливое ожидание неотличимо от «кнопка не сработала».
  for (let attempt = 0; attempt < 15; attempt++) {
    await new Promise((resolve) => setTimeout(resolve, 1000));
    await refreshModels();
    if (lastRunning) break;
  }

  ui.ollamaStart.disabled = false;
  if (!lastRunning) {
    modelsProblem = t("models.ollamaSlow");
    redraw();
  }
});

ui.modelPull.addEventListener("click", () => {
  const name = ui.modelCustom.value.trim();
  if (!name) return;
  ui.modelCustom.value = "";
  startPull(name);
});

// Enter в поле — то же самое, что нажать кнопку: вписал имя, нажал ввод.
ui.modelCustom.addEventListener("keydown", (event) => {
  if (event.key !== "Enter") return;
  event.preventDefault();
  ui.modelPull.click();
});

ui.modelsList.addEventListener("click", async (event) => {
  const pull = event.target.closest("[data-pull]");
  if (pull) {
    startPull(pull.dataset.pull);
    return;
  }

  const pick = event.target.closest("[data-pick]");
  if (!pick) return;
  ui.model.value = pick.dataset.pick;
  try {
    await saveAi();
    ui.aiStatus.textContent = `Модель ${pick.dataset.pick} выбрана. Нажмите «Проверить».`;
  } catch (err) {
    ui.aiStatus.textContent = `Не удалось сохранить: ${err}`;
  }
  await refreshModels();
});

// Ход загрузки идёт событиями из Rust: сотни строк в секунду там сведены
// к шагу в один процент, здесь остаётся только показать.
api?.listen("model:pull", (event) => {
  const { model, percent, status, done, error } = event.payload ?? {};
  if (!model) return;
  if (error) {
    pulling.delete(model);
    ui.modelsHint.textContent = `Не удалось скачать ${model}: ${error}`;
    refreshModels();
    return;
  }
  if (done) {
    pulling.delete(model);
    refreshModels();
    return;
  }
  pulling.set(model, percent > 0 ? `${status} ${percent}%` : `${status}…`);
  renderModels({ running: true, installed: lastInstalled });
});

ui.test.addEventListener("click", async () => {
  ui.aiStatus.textContent = t("action.testing");
  try {
    // Пробуем настоящий запрос на слове «альбедо»: увидеть ответ надёжнее,
    // чем увидеть «ключ принят».
    const answer = await api.invoke("test_ai");
    ui.aiStatus.textContent = `${t("action.works")} ${answer}`;
  } catch (err) {
    ui.aiStatus.textContent = `Не получилось: ${err}`;
  }
});

ui.recheck.addEventListener("click", refresh);
ui.recheckHere.addEventListener("click", refresh);
ui.settings.addEventListener("click", async () => {
  const opened = await api?.invoke("open_permission_settings");
  if (!opened) {
    ui.hint.textContent =
      "Не удалось открыть настройки автоматически — откройте их вручную по инструкции выше.";
  }
});

// Платформа — первой: от неё зависят подписи и то, какие разделы вообще имеют
// смысл. loadSettings идёт следом, потому что список моделей опирается на неё.
// Вид — первым: от языка зависят все подписи, а от темы первый кадр окна.
// Платформа следом: она правит текст низа и прячет разделы, которых на
// телефоне нет. Остальное — уже поверх готового языка.
loadView().then(() => applyPlatform()).then(() => {
  refresh();
  loadSettings();
  loadTrigger();
  refreshCapture();
});
