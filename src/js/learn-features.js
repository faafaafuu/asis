// Что использовать в учёбе: человек сам включает и выключает части —
// ролики, аудиодрамы, собеседование, команды, практику, карточки, конспект.
// Выключенное пропадает из экранов темы и урока. Настройка — на главной
// учёбы; одна и та же в окне на компьютере, на телефоне и в Ноа онлайн
// (хранится на устройстве).

const KEY = "noa.learn.features";

export const FEATURES = [
  { id: "video", name: "Ролики-схемы", about: "Короткое видео по теме и по разделу урока." },
  { id: "drama", name: "Аудиодрамы", about: "История по уроку на три голоса и вопросы после неё." },
  { id: "interview", name: "Собеседование", about: "Ноа задаёт вопросы по теме вслух и разбирает ответы." },
  { id: "commands", name: "Команды для практики", about: "В конце урока — что набрать в терминале и что делает каждая команда." },
  { id: "practice", name: "Практика в терминале", about: "Задача по теме на своём сервере, Ноа ведёт по шагам." },
  { id: "cards", name: "Карточки", about: "Повторение понятий по расписанию." },
  { id: "sheet", name: "Конспект", about: "Вся тема на одной странице." },
];

function read() {
  try {
    return JSON.parse(localStorage.getItem(KEY) ?? "{}") ?? {};
  } catch {
    return {};
  }
}

/** Включена ли часть: по умолчанию — да. */
export function uses(id) {
  return read()[id] !== false;
}

export function setUses(id, on) {
  const all = read();
  if (on) delete all[id];
  else all[id] = false;
  try {
    localStorage.setItem(KEY, JSON.stringify(all));
  } catch {
    /* не запомнится — до перезапуска */
  }
  window.dispatchEvent(new CustomEvent("noa:features", { detail: { id, on } }));
}

/**
 * Блок настройки: переключатели по частям. `el(tag, class, text)` —
 * конструктор окна. `onChange` — перерисовать то, что зависит от настройки.
 */
export function featuresBox({ el, onChange = () => {} }) {
  const box = el("div", "features");
  const list = el("div", "features__list");
  for (const feature of FEATURES) {
    const row = el("label", "features__row");
    const check = el("input", "features__check");
    check.type = "checkbox";
    check.checked = uses(feature.id);
    check.addEventListener("change", () => {
      setUses(feature.id, check.checked);
      onChange(feature.id, check.checked);
    });
    const text = el("span", "features__text");
    text.append(el("span", "features__name", feature.name), el("span", "features__about", feature.about));
    row.append(check, text);
    list.append(row);
  }
  box.append(list);
  return box;
}
