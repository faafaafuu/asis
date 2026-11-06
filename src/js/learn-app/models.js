// Модель приложения — как у Ноа на компьютере: быстрое переключение между
// моделями, на которых она уже работала, и подключение новой по ключу.
// Ключ хранится в настройках программы зашифрованным и никуда, кроме своего
// сервиса, не уходит.

import { el, icon, label, call, sheet, toast } from "./core.js";

/** Готовые сервисы: человек выбирает сервис, а не вписывает адреса. */
const SERVICES = [
  {
    id: "anthropic",
    name: "Claude — ключ Anthropic",
    hint: "Ключ — console.anthropic.com, раздел API Keys. Оплата по факту, курс ≈ $1–2.",
    endpoint: "https://api.anthropic.com/v1/chat/completions",
    model: "claude-sonnet-5-5",
    models: ["claude-sonnet-5-5", "claude-haiku-5-5", "claude-opus-5-5"],
  },
  {
    id: "openrouter",
    name: "OpenRouter",
    hint: "Ключ — openrouter.ai/keys. Есть бесплатные модели и сотни платных.",
    endpoint: "https://openrouter.ai/api/v1/chat/completions",
    model: "z-ai/glm-5.2:free",
  },
  {
    id: "google",
    name: "Gemini — ключ Google",
    hint: "Ключ — aistudio.google.com/apikey. Бесплатно в пределах лимита.",
    endpoint: "https://generativelanguage.googleapis.com/v1beta/openai/chat/completions",
    model: "gemini-flash-latest",
  },
  {
    id: "groq",
    name: "Groq",
    hint: "Ключ — console.groq.com/keys. Быстро, бесплатно в пределах лимита.",
    endpoint: "https://api.groq.com/openai/v1/chat/completions",
    model: "llama-3.3-70b-versatile",
  },
  {
    id: "custom",
    name: "Свой сервис",
    hint: "Любой сервис с OpenAI-совместимым адресом …/chat/completions — например, своя Ollama в сети.",
    endpoint: "",
    model: "",
  },
];

const BRIDGES = {
  "claude-code-bridge": "Claude через NOAH (мост)",
  "codex-bridge": "ChatGPT через NOAH (мост)",
  "gemini-bridge": "Gemini через NOAH (мост)",
  "qwen-bridge": "Qwen через NOAH (мост)",
};

/** Имя модели по-человечески: мост — словами, остальное — без владельца. */
export function modelName(model = "", endpoint = "") {
  const bare = String(model).replace(/:free$/, "");
  if (BRIDGES[bare]) return BRIDGES[bare];
  if (/\/api\/noa\/bridge/.test(endpoint)) return "Claude через NOAH (мост)";
  return bare.split("/").pop() || "не выбрана";
}

function serviceOf(endpoint = "") {
  if (/\/api\/noa\/bridge/.test(endpoint)) return "мост NOAH";
  const found = SERVICES.find((s) => s.endpoint && endpoint.startsWith(s.endpoint.replace(/\/chat\/completions$/, "")));
  if (found) return found.name.replace(/ — .*/, "");
  try {
    return new URL(endpoint).host;
  } catch {
    return "";
  }
}

/** Строка «Модель» в профиле: что сейчас и «Сменить». */
export async function modelRow() {
  const row = el("button", "setting");
  const text = el("span", "setting__text");
  const name = el("span", "setting__name", "Модель…");
  const hint = el("span", "setting__hint", "Через неё Ноа объясняет, проверяет и собирает курсы");
  text.append(name, hint);
  row.append(icon("cpu", 20, 2), text, icon("chevron-right", 18, 2));
  row.firstChild.style.color = "var(--c-accent)";
  const paint = async () => {
    const now = await call("ai_settings").catch(() => null);
    name.textContent = now ? modelName(now.model, now.endpoint) : "Модель не выбрана";
    hint.textContent = now ? `${serviceOf(now.endpoint)} · сменить или подключить свою` : "Подключите модель — без неё Ноа не отвечает";
  };
  row.addEventListener("click", () => openModels(paint));
  await paint();
  return row;
}

/** Лист «Модель»: быстрые варианты и подключение новой. */
export function openModels(changed = () => {}) {
  sheet("Модель", async (content, close) => {
    const list = el("div", "list");
    list.append(label("// на чём работать"));
    const status = el("span", "small muted");
    content.append(list, status);
    const brains = await call("brains_list").catch(() => []);
    for (const brain of brains) {
      const row = el("button", brain.current ? "setting is-current" : "setting");
      const text = el("span", "setting__text");
      text.append(el("span", "setting__name", modelName(brain.model, brain.endpoint)), el("span", "setting__hint", serviceOf(brain.endpoint) || brain.kind));
      row.append(icon(brain.current ? "check" : "cpu", 20, 2), text);
      row.firstChild.style.color = brain.current ? "var(--c-ok)" : "var(--c-soft)";
      row.addEventListener("click", async () => {
        if (brain.current) return;
        status.textContent = "Переключаю…";
        try {
          status.textContent = await call("brains_use", { endpoint: brain.endpoint, model: brain.model });
          await changed();
          close();
          toast(`Модель: ${modelName(brain.model, brain.endpoint)}`);
        } catch (err) {
          status.textContent = String(err);
        }
      });
      list.append(row);
    }
    if (!brains.length) list.append(el("span", "small dim", "Пока ни одной — подключите ниже."));

    content.append(label("// подключить"));
    const pick = el("div", "chips");
    const form = el("div", "list");
    content.append(pick, form);
    for (const service of SERVICES) {
      const chip = el("button", "chip", service.name.replace(/ — .*/, ""));
      chip.addEventListener("click", () => {
        for (const other of pick.children) other.setAttribute("aria-pressed", String(other === chip));
        connect(service, form, async () => {
          await changed();
          close();
        });
      });
      pick.append(chip);
    }
  });
}

/** Форма подключения сервиса: ключ, модель, «Проверить и включить». */
function connect(service, form, done) {
  form.replaceChildren(el("span", "small muted", service.hint));
  const field = (title, input) => {
    const box = el("label", "field-label");
    box.append(title, input);
    form.append(box);
    return input;
  };
  const endpoint = el("input", "field");
  endpoint.placeholder = "https://…/v1/chat/completions";
  endpoint.value = service.endpoint;
  if (service.id === "custom") field("Адрес", endpoint);
  const key = el("input", "field");
  key.type = "password";
  key.autocomplete = "off";
  key.placeholder = service.id === "custom" ? "Ключ, если нужен" : "Ключ сервиса";
  field("Ключ", key);
  const model = el("input", "field");
  model.value = service.model;
  model.placeholder = "Имя модели";
  const options = el("datalist");
  options.id = `models-${service.id}`;
  model.setAttribute("list", options.id);
  for (const name of service.models ?? []) options.append(new Option(name, name));
  field("Модель", model);
  form.append(options);
  const catalog = el("span", "small dim");
  form.append(catalog);
  // Ключ вписан — список моделей сервиса, с ценами: имя наизусть помнить не нужно.
  key.addEventListener("change", async () => {
    if (!key.value.trim() || !endpoint.value.trim()) return;
    catalog.textContent = "Спрашиваю у сервиса список моделей…";
    try {
      const models = await call("cloud_models", { endpoint: endpoint.value.trim(), apiKey: key.value.trim(), proxy: "" });
      options.replaceChildren(...models.slice(0, 300).map((m) => new Option(m.free ? `${m.id} · бесплатно` : m.id, m.id)));
      catalog.textContent = models.length ? `Моделей у сервиса: ${models.length} — начните вводить имя` : "";
    } catch {
      catalog.textContent = "Список моделей не пришёл — впишите имя модели сами.";
    }
  });
  const error = el("span", "error");
  const go = el("button", "btn btn--primary btn--big", "Проверить и включить");
  go.addEventListener("click", async () => {
    if (!endpoint.value.trim()) return (error.textContent = "Впишите адрес сервиса.");
    if (service.id !== "custom" && !key.value.trim()) return (error.textContent = "Впишите ключ сервиса.");
    go.disabled = true;
    error.textContent = "";
    go.textContent = "Проверяю…";
    try {
      await call("save_ai_settings", {
        settings: { provider: "http", endpoint: endpoint.value.trim(), apiKey: key.value.trim(), model: model.value.trim() || service.model, proxy: "" },
      });
      const answer = await call("test_ai", {}, { slow: true, retries: 0 });
      toast(`Модель отвечает: ${String(answer).slice(0, 80)}`, 4000);
      await done();
    } catch (err) {
      error.textContent = `Модель сохранена, но не ответила: ${String(err).replace(/^Error:\s*/, "")}`;
      go.disabled = false;
      go.textContent = "Проверить и включить";
    }
  });
  form.append(error, go);
  key.focus();
}
