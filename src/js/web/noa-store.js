// Где Ноа в браузере держит курсы и прогресс.
//
// Всегда — в этом браузере (localStorage): учиться можно без входа. Вошли на
// сайт — ещё и в аккаунте: курсы, собранные нейросетью по MCP, приходят
// оттуда, а прогресс догоняет вас на другом устройстве. Спор решается
// временем: какой прогресс сохранён позже, тот и верен.

import { createLearning, validate, normalizeCourse } from "./learn-core.js";
import { chat, loadModel } from "./ai-web.js";

const COURSES = "noa.courses";
const PROGRESS = "noa.learning";

const read = (key, fallback) => {
  try {
    return JSON.parse(localStorage.getItem(key) ?? "null") ?? fallback;
  } catch {
    return fallback;
  }
};
const write = (key, value) => {
  try {
    localStorage.setItem(key, JSON.stringify(value));
  } catch {
    // Переполнено или запрещено — прогресс останется в аккаунте, если вошли.
  }
};

async function api(path, options = {}) {
  const response = await fetch(path, {
    method: options.method ?? "GET",
    headers: options.body ? { "Content-Type": "application/json" } : {},
    body: options.body ? JSON.stringify(options.body) : undefined,
    credentials: "same-origin",
  });
  const data = await response.json().catch(() => ({}));
  if (!response.ok) throw new Error(data.error ?? `Ошибка ${response.status}`);
  return data;
}

let opened = null;

/** Ноа этого браузера: курсы, прогресс, обучение. Один на страницу. */
export function openNoa() {
  opened ??= start();
  return opened;
}

async function start() {
  const user = await api("/api/me").then((r) => r.user ?? null).catch(() => null);

  let local = read(COURSES, []);
  let account = [];
  /** Версия курсов в аккаунте: по ней видно, что курс добавили или дописали. */
  let version = "";
  const loadAccount = async () => {
    const data = await api("/api/noa/courses");
    account = data.courses ?? [];
    version = data.version ?? "";
  };
  if (user) await loadAccount().catch(() => {});

  // Прогресс: свой и из аккаунта — берём тот, что сохранён позже.
  const saved = read(PROGRESS, { data: {}, savedAt: 0 });
  let data = saved.data ?? {};
  let savedAt = saved.savedAt ?? 0;
  if (user) {
    const remote = await api("/api/noa/progress").catch(() => null);
    if (remote?.data && (remote.savedAt ?? 0) > savedAt) {
      data = remote.data;
      savedAt = remote.savedAt;
    }
  }

  let pushTimer = 0;
  const store = {
    data,
    save() {
      savedAt = Date.now();
      write(PROGRESS, { data: store.data, savedAt });
      if (!user) return;
      // В аккаунт — не на каждый щелчок, а пачкой через пару секунд.
      clearTimeout(pushTimer);
      pushTimer = setTimeout(() => {
        api("/api/noa/progress", { method: "PUT", body: { data: store.data, savedAt } }).catch(() => {});
      }, 2000);
    },
  };

  const courses = () => {
    // Курс из аккаунта важнее своей копии с тем же id: его обновляет нейросеть.
    const byId = new Map();
    for (const course of local) byId.set(course.id, { ...course, source: "local" });
    for (const course of account) byId.set(course.id, { ...course, source: "account" });
    return [...byId.values()];
  };

  const ai = {
    chat: (messages, opts) => {
      const model = loadModel();
      if (!model) throw new Error("Модель не подключена — подключите её на главной странице Ноа.");
      return chat(model, messages, opts);
    },
  };

  const learning = createLearning({ courses, store, ai });

  return {
    user,
    learning,
    courses,
    /** Добавить курс: из файла или вставленного текста. Отдаёт итог словами. */
    async addCourse(raw) {
      const course = normalizeCourse(raw);
      const problems = validate(course);
      if (problems.length) throw new Error(`Курс не принят:\n- ${problems.join("\n- ")}`);
      if (user) {
        await api("/api/noa/courses", { method: "POST", body: { course: raw } });
        await loadAccount();
      } else {
        local = [...local.filter((c) => c.id !== course.id), raw];
        write(COURSES, local);
      }
      // Замечания о качестве курса нужны его автору — нейросети, которая его
      // собрала (их видит MCP), а не ученику.
      return `Курс «${course.title}» добавлен.`;
    },
    async removeCourse(id) {
      if (account.some((c) => c.id === id)) {
        await api(`/api/noa/courses/${encodeURIComponent(id)}`, { method: "DELETE" });
        account = account.filter((c) => c.id !== id);
      }
      local = local.filter((c) => c.id !== id);
      write(COURSES, local);
    },
    /** Перечитать курсы аккаунта: нейросеть могла собрать новый. */
    /**
     * Перечитать курсы аккаунта, если они поменялись: нейросеть по MCP или
     * сборка дописали тему. Отдаёт true, если что-то новое пришло. Сначала
     * спрашивается только версия — список весит сотни килобайт.
     */
    async refresh() {
      if (!user) {
        // Гость: курсы в этом браузере — их могла дописать сборка в другой вкладке.
        const now = read(COURSES, []);
        const mark = (all) => all.map((c) => `${c.id}:${c.topics?.length ?? 0}:${c.building?.done ?? "-"}`).join("|");
        if (mark(now) === mark(local)) return false;
        local = now;
        return true;
      }
      try {
        const { version: now } = await api("/api/noa/courses/version");
        if (now === version) return false;
        await loadAccount();
        return true;
      } catch {
        return false;
      }
    },
    /** Сборки курсов на сервере — последние пять. */
    async builds() {
      if (!user) return [];
      const data = await api("/api/noa/builds").catch(() => ({ builds: [] }));
      return data.builds ?? [];
    },
    /** Собрать курс на сервере через мост. */
    async startBuild(goal, model, quality) {
      const { build } = await api("/api/noa/builds", { method: "POST", body: { goal, model, quality } });
      return build;
    },
    async stopBuild(id) {
      await api(`/api/noa/builds/${encodeURIComponent(id)}/stop`, { method: "POST", body: {} });
    },
    /** Методика курса — для сборки в этом браузере. */
    async courseFormat() {
      const { text } = await api("/api/noa/course-format");
      return text ?? "";
    },
    /** Сохранить недособранный курс как есть — для сборки в этом браузере. */
    async saveBuilding(raw) {
      if (user) {
        await api("/api/noa/courses", { method: "POST", body: { course: raw } });
        await loadAccount();
      } else {
        local = [...local.filter((c) => c.id !== raw.id), raw];
        write(COURSES, local);
      }
    },
  };
}
