// Скрипт окна попапа в приложении (десктоп и мобильные сборки Tauri).
//
// Отличие от веб-режима: здесь попап — целое окно, поэтому позиционированием
// занимается Rust. Фронтенд отвечает только за содержимое и сообщает свой размер.

import { PopupView } from "./popup-view.js";
import { TauriProvider, MockProvider, DEFAULT_ERROR_TEXT } from "./ai-client.js";
import { tauri, applyTheme } from "./bridge.js";
import { attachMobileEntry } from "./mobile-entry.js";

/** Поле вокруг попапа внутри окна — под тень (SPEC §5). Должно совпадать с CSS. */
const SHADOW_INSET = 48;

const api = tauri();
const mount = document.getElementById("mount");

const view = new PopupView({
  // Без Tauri (открыли popup.html в браузере) — mock, чтобы окно не было пустым.
  client: api ? new TauriProvider(api.invoke) : new MockProvider(),
  errorText: DEFAULT_ERROR_TEXT,
  onGeometry: ({ width, height }) => {
    if (!api || !width || !height) return;
    api.invoke("popup_ready", { width, height, shadowInset: SHADOW_INSET }).catch(() => {});
  },
  onClose: () => close(),
});

mount.append(view.el);

function close() {
  view.close();
  api?.invoke("close_popup").catch(() => {});
}

if (api) {
  // Тему Rust проставил до первого кадра — здесь её не трогаем вовсе.
  // Попап всплывает поверх чужого окна мгновенно, и вспышка чужой темы
  // заметна именно тут.
  api
    .invoke("runtime_config")
    .then((config) => {
      view.errorText = config.errorText || DEFAULT_ERROR_TEXT;
      view.dialogue = Boolean(config.dialogue);
    })
    .catch(() => {});

  /** Показывает содержимое по данным из Rust. */
  function applyOpen(payload) {
    const { term, context, theme, errorText, dialogue, speak } = payload ?? {};
    if (theme) applyTheme(theme);
    if (errorText) view.errorText = errorText;
    if (dialogue !== undefined) view.dialogue = Boolean(dialogue);
    view.open({ term: term ?? "", context: context ?? "", speak: Boolean(speak) });
  }

  // Окно могли открыть раньше, чем эта страница загрузилась: тогда событие
  // до нас не дошло, и Rust придержал вопрос до этого запроса. Без него первое
  // открытие висело бы с вечным «Анализирую…».
  api
    .invoke("pending_open")
    .then((payload) => {
      if (payload) applyOpen(payload);
    })
    .catch(() => {
      /* окно открыто вне приложения — показывать нечего */
    });

  // Новое выделение: Rust уже сохранил якорь, нам остаётся показать содержимое.
  api.listen("popup:open", (event) => {
    const { term, context, theme, errorText, dialogue, speak } = event.payload ?? {};
    if (theme) applyTheme(theme);
    if (errorText) view.errorText = errorText;
    // Источник могли сменить в настройках, пока окно жило: сведения о том, есть
    // ли кому отвечать на «?», приходят с каждым открытием.
    if (dialogue !== undefined) view.dialogue = Boolean(dialogue);
    view.open({ term: term ?? "", context: context ?? "", speak: Boolean(speak) });
  });

  // Пробел: прочитать вслух. Клавишу ловит и забирает себе Rust — окно
  // намеренно не держит фокус, и до него нажатия не доходят (SPEC §8).
  api.listen("voice:speak", () => {
    const text = view.spokenText();
    if (!text) return;
    api.invoke("voice_speak", { text }).catch(() => {
      // Голос не скачан или выключен. Молча: попап живёт секунды, и ругаться
      // на него поверх чужого окна незачем — состояние видно в настройках.
    });
  });

  // Расшифрованный вопрос: кладём в тред и ждём ответа.
  api.listen("voice:question", (event) => {
    view.askByVoice(event.payload);
  });

  // Ответ на голосовой вопрос читаем вслух — круг замыкается.
  view.onAnswer = (answer) => {
    api.invoke("voice_speak", { text: answer }).catch(() => {});
  };

  // Идёт запись голоса — показываем, что слушаем.
  api.listen("voice:listening", (event) => {
    view.listening = Boolean(event.payload);
  });

  // На мобильных вход другой: текст приходит из нативного плагина.
  attachMobileEntry(api, (term) => view.open({ term, context: "" }));
} else {
  applyTheme("system");
  view.open({ term: "альбедо", context: "" });
}

// Esc внутри окна: работает, когда фокус всё-таки здесь (пользователь кликнул в поле
// «Спросить ещё…»). Esc при фокусе в чужом приложении ловит Rust — окно намеренно
// не забирает фокус, и до него клавиатурные события не доходят (SPEC §8).
document.addEventListener(
  "keydown",
  (e) => {
    if (e.key === "Escape") close();
  },
  true,
);
