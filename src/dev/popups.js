// Витрина попапа: каждое состояние из дизайна — отдельный экземпляр с
// подменным клиентом. Ничего не ходит в сеть.

import { PopupView } from "../js/popup-view.js";
import { AiError } from "../js/ai-client.js";

const THEMES = [
  ["noah", "NOAH", "#eceef2", "#171b26"],
  ["system", "Системная", "#f4f1ea", "#221f1b"],
  ["light", "Светлая", "#efeae0", "#221f1b"],
  ["dark", "Тёмная", "#221e1a", "#f2ece1"],
  ["neon", "Неон", "#05070b", "#d8f7fa"],
  ["synthwave", "Синтвейв", "#12081c", "#fbe9f6"],
];

const BASE = {
  def: "Кабель из медных жил, скрученных попарно. Скрутка гасит наводки — поэтому сеть по нему идёт до 100 м без усилителя.",
  simple: "",
  examples: ["сеть в офисе и дома — розетка RJ-45", "камеры с питанием по тому же кабелю (PoE)"],
};

const LONG =
  "Процесс, где результат влияет на свою же причину. Бывает двух видов.\n\n" +
  "**Усиливающая.** Меньше льда — темнее поверхность — больше тепла — ещё меньше льда. Так таяние ускоряет само себя.\n\n" +
  "**Гасящая.** Термостат: теплее нужного — отопление выключается, холоднее — включается. Система держится у цели.\n\n" +
  "В климате обе работают одновременно, и прогнозы спорят как раз о том, какая перевесит.\n\n" +
  "Облака — пример спорной связи: днём они отражают солнце и охлаждают, ночью держат тепло.\n\n" +
  "Океан поглощает углекислый газ, но чем он теплее, тем хуже растворяет газ.";

const CODE =
  "Команда: стучится до адреса и ждёт отклика.\n```cmd\nping -n 4 ya.ru\n```\n" +
  "| Видите | Значит |\n|---|---|\n| время=12мс | связь есть |\n| превышен интервал | адрес молчит |\n\n" +
  "1. сначала пингуйте роутер, потом сайт";

/** Клиент, который отвечает тем, что нужно состоянию. */
function client({ answer = BASE, fail = null, hang = false } = {}) {
  return {
    explain: () => (hang ? new Promise(() => {}) : fail ? Promise.reject(fail) : Promise.resolve(answer)),
    ask: () => Promise.resolve("Ответ на уточнение."),
  };
}

function popup(opts, setup) {
  const view = new PopupView({ client: client(opts), dialogue: true, errorText: "Сбой сети — нет ответа" });
  view.onSpeak = () => {};
  view.onStopSpeaking = () => {};
  view.onMic = () => {};
  view.onOpenSettings = () => {};
  setup(view);
  return view.el;
}

function cell(label, node, fg) {
  const box = document.createElement("div");
  box.className = "cell";
  const caption = document.createElement("span");
  caption.textContent = label;
  caption.style.color = fg;
  box.append(caption, node);
  return box;
}

const STATES = [
  ["ответ · «?» раскрыт", {}, (v) => v.open({ term: "витая пара" }) || setTimeout(() => v.expand({ elaborate: false }))],
  ["1 · думает", { hang: true }, (v) => v.open({ term: "изостазия" })],
  ["3 · читает вслух", { answer: { def: "Равновесие земной коры на вязкой мантии. Кора плавает, как плот: снимите с неё груз ледника — и она поднимется. Скандинавия так растёт до сих пор.", examples: [] } }, (v) => {
    v.open({ term: "изостазия" });
    setTimeout(() => (v.speaking = true));
  }],
  ["4 · длинный ответ", { answer: { def: LONG, examples: [] } }, (v) => v.open({ term: "обратная связь" })],
  ["5 · код, таблица, список", { answer: { def: CODE, examples: [] } }, (v) => v.open({ term: "ping" })],
  ["6 · ошибка модели", { fail: new AiError("Модель не ответила — ключ OpenRouter не подошёл. Скопируйте ключ заново в кабинете openrouter.ai → Keys и вставьте в «Настройках».", { kind: "backend", status: 401 }) }, (v) => v.open({ term: "изостазия" })],
  ["7 · нет сети", { fail: new AiError("fetch failed", { kind: "network" }) }, (v) => v.open({ term: "изостазия" })],
  ["8 · весть", {}, (v) => v.announce("Вышла версия 1.14. Поставится сама, когда вы отойдёте от компьютера.", { kind: "news" })],
  ["8 · напоминание", {}, (v) => v.announce("Позвонить в банк про карту.", { kind: "reminder", label: "Напоминание · 15:00" })],
  ["остановлено", { hang: true }, (v) => {
    v.open({ term: "изостазия" });
    setTimeout(() => v.stop());
  }],
];

const only = new URLSearchParams(location.search).get("theme");
const gallery = document.getElementById("gallery");
for (const [id, name, stage, fg] of THEMES) {
  if (only && only !== id) continue;
  const title = document.createElement("h2");
  title.textContent = `${name} · ${id}`;
  const row = document.createElement("div");
  row.className = "row stage";
  row.dataset.theme = id;
  row.style.background = stage;
  const states = only || id === "noah" || id === "dark" ? STATES : STATES.slice(0, 1);
  for (const [label, opts, setup] of states) row.append(cell(label, popup(opts, setup), fg));
  gallery.append(title, row);
}
