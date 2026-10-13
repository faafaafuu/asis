// Суфлёр в приложении: выделили слово или фразу — рядом «Объяснить», и Ноа
// объясняет его в смысле абзаца, как в Ноа онлайн и в программе на
// компьютере. В окне объяснения — тот же чат: уточнить текстом или голосом
// (зажать микрофон), прочитать вслух. Объясняет модель программы
// (ai_explain / ai_ask) — через мост сайта.

import { WebHost } from "../web-host.js";
import { api, voice, isWeb } from "./core.js";

/** Абзац вокруг выделения — чтобы слово объяснялось в своём смысле. */
function selectionContext() {
  const node = getSelection()?.anchorNode;
  const block = (node?.nodeType === 1 ? node : node?.parentElement)?.closest("p, li, td, pre, h3, h4, blockquote, .card, .concept, .say, div");
  return (block?.textContent ?? "").replace(/\s+/g, " ").trim().slice(0, 800);
}

let host = null;

export function mountSuflyor() {
  if (host || !api) return host;
  const client = {
    explain: (term, context) => api.invoke("ai_explain", { term, context }),
    ask: (term, context, thread, question) =>
      api.invoke("ai_ask", { term, context, thread: (thread ?? []).map((item) => ({ q: item.q, a: item.a })), question }),
  };
  host = new WebHost({
    client,
    // На телефоне — мини-меню по выделению, без клавиш.
    requireLeftCtrl: true,
    forceTouchMenu: true,
    onRead: (text) => voice.speak(text),
    onStopRead: () => voice.stop(),
  }).mount();

  const view = host.view;
  view.dialogue = true;
  view.onSpeak = (text) => {
    view.speaking = true;
    voice.speak(text).finally(() => (view.speaking = false));
  };
  view.onStopSpeaking = () => {
    voice.stop();
    view.speaking = false;
  };
  // Микрофон зажат — слушаем; отпустили — фраза уходит вопросом.
  let hearing = null;
  view.onMic = async (down) => {
    if (down) {
      voice.stop();
      view.listening = true;
      hearing = voice.listen().catch(() => "");
      return;
    }
    view.listening = false;
    api.invoke("plugin:sufler|stopListening").catch(() => {});
    const text = await (hearing ?? Promise.resolve(""));
    hearing = null;
    if (text) view.askByVoice(text);
  };
  view.onAnswer = (answer) => {
    if (voice.aloud()) view.onSpeak(answer);
  };
  const showAt = host.showAt.bind(host);
  host.showAt = (anchor, term, context = "") => showAt(anchor, term, context || selectionContext());
  quietSystemMenu();
  return host;
}

/**
 * Системное меню выделения iPhone («Скопировать», «Перевести»…) плагин
 * выключает: над текстом — меню суфлёра. В поле ввода оно нужно (вставить,
 * вырезать) — окно сообщает, когда фокус в поле.
 */
function quietSystemMenu() {
  if (isWeb) return;
  const editable = (node) =>
    Boolean(node?.isConnected && node.matches?.("input, textarea, select, [contenteditable]:not([contenteditable='false'])"));
  let editing = null;
  const tell = () => {
    const now = editable(document.activeElement);
    if (now === editing) return;
    const first = editing === null;
    editing = now;
    api
      .invoke("plugin:sufler|systemMenu", { editing: now })
      .then((answer) => {
        // Один раз за запуск — что подменилось: меню собирается по-разному в разных iOS.
        if (first) api.invoke("learn_diag", { line: `меню выделения: ${answer?.hooked || "не подменено"}` }).catch(() => {});
      })
      .catch(() => {});
  };
  tell();
  document.addEventListener("focusin", tell);
  document.addEventListener("focusout", () => setTimeout(tell, 0));
  // Поле ушло со страницы в фокусе (закрыли лист) — focusout не приходит.
  document.addEventListener("selectionchange", tell);
  document.addEventListener("touchstart", tell, { passive: true, capture: true });
}
