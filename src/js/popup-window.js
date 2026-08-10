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
  // Тема и текст ошибки приходят из конфигурации приложения.
  api
    .invoke("runtime_config")
    .then((config) => {
      applyTheme(config.theme);
      view.errorText = config.errorText || DEFAULT_ERROR_TEXT;
      view.dialogue = Boolean(config.dialogue);
    })
    .catch(() => applyTheme("system"));

  // Новое выделение: Rust уже сохранил якорь, нам остаётся показать содержимое.
  api.listen("popup:open", (event) => {
    const { term, context, theme, errorText, dialogue } = event.payload ?? {};
    if (theme) applyTheme(theme);
    if (errorText) view.errorText = errorText;
    // Источник могли сменить в настройках, пока окно жило: сведения о том, есть
    // ли кому отвечать на «?», приходят с каждым открытием.
    if (dialogue !== undefined) view.dialogue = Boolean(dialogue);
    view.open({ term: term ?? "", context: context ?? "" });
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
