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

/* ── Для каждого курса — своё ───────────────────────────────────────────
   При сборке человек отмечает, что нужно в курсе: в матанализе терминал ни к
   чему. Не отмечал — практика и команды есть только в технических курсах
   (программирование, Linux, сети, DevOps, базы), остальное — как в общей
   настройке. Общая настройка «выключено» сильнее всего. */

const COURSE_KEY = "noa.learn.courseUses";
/** Части, которые к нетехническому курсу не нужны. */
const TECH_ONLY = new Set(["practice", "commands"]);
const TECH =
  /linux|unix|bash|shell|терминал|командн|docker|kubernetes|k8s|devops|sre|сет(и|ь|ей|евы)|network|tcp|dns|http|nginx|сервер|server|cloud|облак|ansible|terraform|\bgit\b|python|java|javascript|typescript|golang|\bgo\b|rust|c\+\+|c#|php|ruby|kotlin|swift|sql|postgres|mysql|баз[аы] данных|database|программ|разработ|backend|frontend|бэкенд|фронтенд|\bapi\b|безопасност|security|pentest|кибер|нод[аыу]\b|блокчейн|blockchain|ethereum|bitcoin|solana|администр|сисадмин|\bqa\b|тестировщ|автоматизац|data engineer|airflow|spark|kafka|redis|mongo|1с/i;

function readCourses() {
  try {
    return JSON.parse(localStorage.getItem(COURSE_KEY) ?? "{}") ?? {};
  } catch {
    return {};
  }
}

/** Похож ли курс на технический: по названию, описанию и темам. */
export function technical(course) {
  if (!course || typeof course !== "object") return false;
  const text = [course.title, course.description, ...(course.topics ?? []).map((t) => t.title)].join(" ");
  return TECH.test(text);
}

/** Что выбрано для курса при сборке (или позже): { video: false, … }. */
export function courseChoice(courseId) {
  return readCourses()[courseId] ?? null;
}

/** Запомнить выбор для курса — целиком. */
export function setCourseChoice(courseId, choice) {
  if (!courseId) return;
  const all = readCourses();
  all[courseId] = { ...choice };
  try {
    localStorage.setItem(COURSE_KEY, JSON.stringify(all));
  } catch {
    /* не запомнится — до перезапуска */
  }
  window.dispatchEvent(new CustomEvent("noa:features", { detail: { course: courseId } }));
}

/**
 * Включена ли часть. `course` — курс (объект или id): тогда учитывается
 * его выбор, а практика и команды — только в технических курсах.
 */
export function uses(id, course = null) {
  if (read()[id] === false) return false;
  if (!course) return true;
  const courseId = typeof course === "object" ? course.id : course;
  const own = courseChoice(courseId)?.[id];
  if (own !== undefined) return own !== false;
  if (TECH_ONLY.has(id) && typeof course === "object") return technical(course);
  return true;
}

/** Переключатели для одного курса: что в нём показывать. */
export function courseFeaturesBox({ el, course, onChange = () => {} }) {
  const list = el("div", "features__list");
  const choice = Object.fromEntries(FEATURES.map((f) => [f.id, uses(f.id, course)]));
  for (const feature of FEATURES) {
    const row = el("label", "features__row");
    const check = el("input", "features__check");
    check.type = "checkbox";
    check.checked = choice[feature.id];
    check.addEventListener("change", () => {
      choice[feature.id] = check.checked;
      setCourseChoice(course.id, choice);
      onChange(feature.id, check.checked);
    });
    const text = el("span", "features__text");
    text.append(el("span", "features__name", feature.name), el("span", "features__about", feature.about));
    row.append(check, text);
    list.append(row);
  }
  return list;
}

/** Предложение по цели сборки: что включить в новый курс. */
export function suggestChoice(goal) {
  const tech = TECH.test(String(goal ?? ""));
  return Object.fromEntries(FEATURES.map((f) => [f.id, TECH_ONLY.has(f.id) ? tech : true]));
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
