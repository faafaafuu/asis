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
    ui.apiKey.placeholder = settings.apiKey ? "ключ сохранён — оставьте пустым" : "вставьте ключ";
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
  ui.aiStatus.textContent = "Сохраняю…";
  try {
    await saveAi();
    ui.aiStatus.textContent = "Сохранено. Нажмите «Проверить», чтобы убедиться, что работает.";
  } catch (err) {
    ui.aiStatus.textContent = `Не удалось сохранить: ${err}`;
  }
});

/* ── Модели на этом компьютере ──────────────────────────────────────────── */

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

  // Два яруса, а не один ряд: имя модели и её состояние сверху, пояснение
  // строкой ниже. В один ряд пояснение зажималось между именем и кнопкой,
  // ломалось на три строки, и размер отрывался от текста, к которому относится.
  const head = document.createElement("div");
  head.className = "ob__model-head";

  const title = document.createElement("span");
  title.className = "ob__model-name";
  title.textContent = name;

  // Размер показываем всегда, а не только у нескачанных: у скачанной он
  // отвечает на вопрос «сколько это занимает у меня на диске». В отличие от
  // пояснения это не справка, а часть решения — его не прячем.
  const sizeEl = document.createElement("span");
  sizeEl.className = "ob__model-size";
  sizeEl.textContent = size;

  head.append(title, sizeEl);

  const hint = document.createElement("span");
  hint.className = "ob__model-note";
  hint.textContent = note;
  // Наведение показывает пояснение и без раскрытия списка: тому, кто просто
  // хочет узнать про одну строку, незачем разворачивать все девять.
  if (note) row.title = note;

  row.append(head);
  if (note) row.append(hint);

  if (pulling.has(name)) {
    const progress = document.createElement("span");
    progress.className = "ob__model-state";
    progress.textContent = pulling.get(name);
    head.append(progress);
    return row;
  }

  if (installed) {
    // Выбор — щелчок по строке целиком, а не по крошечной галочке: строк мало,
    // промахнуться не по чему.
    row.tabIndex = 0;
    row.dataset.pick = name;
    const state = document.createElement("span");
    state.className = "ob__model-state";
    // «Скачана» вместо молчания: раньше строка без кнопки выглядела так же,
    // как строка с кнопкой, и понять, что уже есть на диске, было нельзя.
    state.textContent = chosen ? "✓ выбрана" : "скачана — выбрать";
    head.append(state);
    return row;
  }

  const button = document.createElement("button");
  button.type = "button";
  button.className = "ob__btn ob__btn--slim";
  button.dataset.pull = name;
  button.textContent = "Скачать";
  head.append(button);
  return row;
}

function renderModels(status) {
  ui.modelsList.replaceChildren();
  ui.modelsList.dataset.notes = showModelNotes ? "on" : "off";
  ui.modelsInfo.textContent = showModelNotes ? "скрыть пояснения" : "чем отличаются";
  lastInstalled = status?.installed ?? lastInstalled;
  lastRunning = status?.running ?? false;

  if (!status?.running) {
    // Установлена, но молчит — обычное дело после перезагрузки: Ollama не
    // всегда прописывается в автозапуск. Советовать «установите с ollama.com»
    // тому, у кого она уже стоит, — значит переложить вину на человека и не
    // сказать, что делать. Предлагаем запустить прямо отсюда.
    ui.ollamaStart.hidden = !status?.present;
    ui.modelsHint.textContent = status?.present
      ? "Ollama установлена, но не запущена — скачанные модели не видны, пока она молчит."
      : "Ollama не найдена. Установите её с ollama.com — программа сама увидит.";
    return;
  }

  ui.ollamaStart.hidden = true;

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

  ui.modelsToggle.textContent = showAllModels ? "Свернуть список" : "Показать другие модели";
  ui.modelAdd.hidden = !showAllModels;

  // Сообщение о неудаче переживает перерисовку и держится, пока человек не
  // начнёт новое действие. Раньше его писали прямо в подсказку, а следующая
  // же строка этой функции затирала текст дежурным пояснением — сообщение
  // жило миллисекунды, и со стороны нажатие «Скачать» выглядело так, будто
  // кнопка ничего не делает. Именно так и терялась ошибка «Ollama не отвечает».
  if (modelsProblem) {
    ui.modelsHint.textContent = modelsProblem;
  } else if (pulling.size) {
    ui.modelsHint.textContent = "Загрузка идёт в фоне — окно можно закрыть, она не прервётся.";
  } else if (installed.size === 0) {
    ui.modelsHint.textContent =
      "Ни одной модели пока нет. Скачайте любую — это разовое действие, дальше она работает без интернета.";
  } else {
    ui.modelsHint.textContent =
      "Модель скачивается один раз и дальше работает без интернета. Объяснения берутся из выбранной.";
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
  pulling.set(name, "готовлюсь…");
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

ui.ollamaStart.addEventListener("click", async () => {
  modelsProblem = "";
  ui.ollamaStart.disabled = true;
  ui.modelsHint.textContent = "Запускаю Ollama…";
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
    modelsProblem = "Ollama запущена, но пока не отвечает. Подождите немного и нажмите «Обновить».";
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
ui.recheckHere.addEventListener("click", refresh);
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
