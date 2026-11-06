// Сборка курса прямо в приложении — на модели, которую человек подключил в
// Профиль → Модель (Ollama, OpenRouter, Claude, Gemini, Groq, свой сервис или
// мост аккаунта), как в Ноа. Тот же сборщик и тот же стандарт курса, что в
// Ноа онлайн и по MCP: план → тема за темой урок, понятия, карточки, задачи,
// экзамен → финальный экзамен. Каждая готовая тема сразу ложится на
// устройство и в аккаунт — учиться можно с первой.
//
// Сборка живёт, пока открыто приложение. Закрыли посередине — карточка
// покажет «остановлен», «Продолжить» доберёт со следующей темы.

import { buildCourse as runBuilder, Stopped } from "../web/course-builder.js";
import { call, prefs } from "./core.js";
import { setCourseChoice } from "../learn-features.js";
import { modelName } from "./models.js";

const KEY = "localBuild";
let current = null;
const listeners = new Set();

function changed() {
  for (const listener of [...listeners]) listener();
}

/** Подписка на ход сборки: карточка на главной перерисовывается сама. */
export function onBuild(listener) {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

/** Сохранённое состояние: прерванная сборка переживает перезапуск. */
function saved() {
  const state = prefs.get(KEY);
  return state && typeof state === "object" ? state : null;
}

function remember(state) {
  if (state) prefs.set(KEY, state);
  else prefs.set(KEY, null);
}

/** Сборки на устройстве — в том же виде, что серверные: для карточки. */
export function localBuilds() {
  const state = current?.state ?? saved();
  if (!state) return [];
  // Шла, а приложение закрыли — теперь она остановлена.
  const status = !current && state.status === "running" ? "stopped" : state.status;
  const message = !current && state.status === "running" ? "Приложение закрылось посередине — готовые темы остались." : state.message;
  return [{ ...state, id: "local", local: true, status, message }];
}

export const isBuilding = () => current?.state.status === "running";

function start({ goal, resume = null, choice = null, depth = null }) {
  if (isBuilding()) throw new Error("Уже собирается курс — дождитесь его или остановите.");
  const now = Date.now();
  const control = {
    stop: false,
    state: {
      status: "running",
      goal,
      depth: depth ?? "full",
      title: resume?.title ?? "",
      message: "Составляю план курса",
      done: resume?.topics?.length ?? 0,
      total: resume?.building?.total ?? 0,
      courseId: resume?.id ?? "",
      building: resume?.building ?? null,
      // Что видно в карточке: сколько идёт сборка, на какой модели и сколько
      // модель думает над текущим шагом — чтобы было ясно, жива ли сборка.
      startedAt: now,
      stepAt: now,
      waitingSince: null,
      answers: 0,
      model: "",
    },
  };
  current = control;
  remember(control.state);
  changed();
  // Пока идёт — карточка обновляет время сама.
  const ticker = setInterval(() => current === control && changed(), 5000);
  call("ai_settings")
    .then((ai) => {
      control.state.model = ai?.endpoint ? modelName(ai.model, ai.endpoint) : "";
      changed();
    })
    .catch(() => {});
  const chat = async (messages, opts = {}) => {
    const system = messages.find((m) => m.role === "system")?.content ?? "";
    const said = messages.filter((m) => m.role !== "system").map((m) => m.content).join("\n\n");
    control.state.waitingSince = Date.now();
    changed();
    try {
      return await call("learn_compose", { rules: system, said, maxTokens: opts.maxTokens ?? 4000, json: Boolean(opts.json) }, { retries: 0, slow: true });
    } finally {
      control.state.waitingSince = null;
      control.state.answers += 1;
      changed();
    }
  };
  (async () => {
    const format = await call("learn_course_format", {}, { retries: 1 });
    const taken = new Set((await call("learn_overview").catch(() => []))?.map?.((c) => c.id) ?? []);
    if (resume?.id) taken.delete(resume.id);
    await runBuilder({
      goal,
      format,
      chat,
      // Курс с полем building — недособранный: оно нужно, чтобы продолжить.
      save: async (course) => {
        const { building, ...clean } = course;
        await call("learn_build_save", { course: clean }, { retries: 2, slow: true });
        // Помним только план: готовые темы лежат на устройстве.
        control.state.building = building ?? null;
        // Что выбрано для курса при сборке — как только известен его id.
        if (choice && control.state.courseId !== course.id) setCourseChoice(course.id, choice);
        control.state.courseId = course.id;
        control.state.title = course.title;
        remember(control.state);
        changed();
      },
      progress: (p) => {
        if (p.message !== control.state.message) control.state.stepAt = Date.now();
        Object.assign(control.state, { message: p.message, done: p.done, total: p.total, title: p.title ?? control.state.title, courseId: p.courseId ?? control.state.courseId });
        remember(control.state);
        changed();
      },
      stopped: () => control.stop,
      resume,
      taken,
      depth: control.state.depth,
    });
  })()
    .then(() => {
      control.state.status = "done";
      control.state.message = "Курс готов.";
      control.state.building = null;
    })
    .catch((err) => {
      const stopped = err instanceof Stopped;
      control.state.status = stopped ? "stopped" : "failed";
      control.state.message = stopped ? "Сборка остановлена — готовые темы остались." : String(err?.message ?? err).replace(/^Error:\s*/, "");
    })
    .finally(() => {
      clearInterval(ticker);
      control.state.waitingSince = null;
      // Остановили раньше — состояние уже записано, новое не трогаем.
      if (current !== control) return;
      current = null;
      // Готовый курс — карточка сборки больше не нужна.
      remember(control.state.status === "done" ? null : control.state);
      changed();
    });
}

/** Собрать курс по цели — моделью из настроек. */
export function buildLocal(goal, choice = null, depth = "full") {
  start({ goal, choice, depth });
}

/**
 * Остановить — сразу: карточка показывает «остановлена», а ответ модели на
 * текущий шаг, если придёт, в дело не пойдёт.
 */
export function stopLocal() {
  const control = current;
  if (!control) return;
  control.stop = true;
  control.state.status = "stopped";
  control.state.waitingSince = null;
  control.state.message = "Сборка остановлена — готовые темы остались.";
  current = null;
  remember(control.state);
  changed();
}

/** Продолжить прерванную — со следующей темы. */
export async function resumeLocal() {
  const state = saved();
  if (!state) throw new Error("Нечего продолжать.");
  // Плана ещё не было — собираем заново по той же цели.
  if (!state.building?.plan || !state.courseId) return start({ goal: state.goal, depth: state.depth });
  const course = await call("learn_course_raw", { id: state.courseId }, { retries: 1 });
  start({ goal: state.goal, depth: state.depth, resume: { ...course, building: state.building } });
}

/** Убрать карточку: готовые темы остаются курсом. */
export function removeLocal() {
  if (isBuilding()) throw new Error("Сначала остановите сборку.");
  remember(null);
  changed();
}

/** Остановить, продолжить или убрать сборку — на устройстве или на сервере. */
export async function buildAction(build, action) {
  if (build.local) {
    if (action === "stop") return stopLocal();
    if (action === "resume") return resumeLocal();
    return removeLocal();
  }
  const command = { stop: "learn_build_stop", resume: "learn_build_resume", remove: "learn_build_remove" }[action];
  return call(command, { id: build.id }, { slow: true, retries: 0 });
}
