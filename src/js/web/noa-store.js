// Где Ноа в браузере держит курсы и прогресс.
//
// Всегда — в этом браузере (localStorage): учиться можно без входа. Вошли на
// сайт — ещё и в аккаунте: курсы, собранные нейросетью по MCP, приходят
// оттуда, а прогресс догоняет вас на другом устройстве. Спор решается
// временем: какой прогресс сохранён позже, тот и верен.

import { createLearning, validate, normalizeCourse, mergeProgress } from "./learn-core.js";
import { chat, loadModel } from "./ai-web.js";
import { request } from "./net.js";
import { remembered, whoIsIn } from "./early.js";

const COURSES = "noa.courses";
const PROGRESS = "noa.learning";
const REMOVED = "noa.removed";

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
  // Чтение повторяем при сбое сети; запись — нет: повтор POST мог бы
  // сделать дело дважды.
  const reading = (options.method ?? "GET") === "GET";
  const response = await request(path, {
    method: options.method ?? "GET",
    headers: options.body ? { "Content-Type": "application/json" } : {},
    body: options.body ? JSON.stringify(options.body) : undefined,
    timeout: path.startsWith("/api/noa/courses") && !path.endsWith("/version") ? 45_000 : 15_000,
    retries: reading ? 2 : 0,
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
  // Кто вошёл — общий ответ страницы. Сеть подвела — тот, кто входил в
  // прошлый раз: его курсы есть в памяти браузера. Раньше сбой сети молча
  // делал человека гостем, и курсы аккаунта не появлялись вовсе.
  const answer = await whoIsIn();
  const user = answer === undefined ? remembered : answer;

  let local = read(COURSES, []);
  let account = [];
  /** Версия курсов в аккаунте: по ней видно, что курс добавили или дописали. */
  let version = "";
  /**
   * Курсы аккаунта лежат и в этом браузере — вместе с версией. При заходе
   * сверяется только версия; совпала — курсы берутся отсюда. Иначе каждый
   * заход тянул бы сотни килобайт, а по мобильной сети это секунды.
   */
  const CACHE = user ? `noa.account.${user.id}` : "";
  const loadAccount = async () => {
    const data = await api("/api/noa/courses");
    account = data.courses ?? [];
    version = data.version ?? "";
    write(CACHE, { version, courses: account });
  };
  if (user) {
    const cached = read(CACHE, null);
    const now = await api("/api/noa/courses/version").then((r) => r.version).catch(() => null);
    if (cached?.version && cached.version === now) {
      account = cached.courses ?? [];
      version = cached.version;
    } else {
      await loadAccount().catch(() => {
        // Сеть подвела — лучше вчерашние курсы, чем пустой список.
        account = cached?.courses ?? [];
      });
    }
  }

  // Прогресс: свой и из аккаунта — берём тот, что сохранён позже.
  const saved = read(PROGRESS, { data: {}, savedAt: 0 });
  let data = saved.data ?? {};
  let savedAt = saved.savedAt ?? 0;
  /** До какого сохранения прогресс уже лежит в аккаунте. */
  let pushedAt = saved.pushedAt ?? 0;
  let remoteAt = 0;
  // С другими устройствами — по курсам: у каждого остаётся более свежий.
  // Программа на компьютере шлёт свой прогресс туда же, поэтому здесь и
  // появляется, где вы остановились в ней.
  let localAhead = false;
  if (user) {
    const remote = await api("/api/noa/progress").catch(() => null);
    remoteAt = remote?.savedAt ?? 0;
    if (remote?.data) {
      const merged = mergeProgress(remote.data, data);
      localAhead = JSON.stringify(merged.courses) !== JSON.stringify(remote.data.courses ?? {});
      data = merged;
      if (!localAhead) pushedAt = Math.max(pushedAt, savedAt);
    }
  }
  /** Пришло ли с других устройств что-то, чего окно ещё не показало. */
  let arrived = false;
  const adopt = (remoteData) => {
    if (!remoteData) return;
    const merged = mergeProgress(remoteData, store.data);
    if (JSON.stringify(merged.courses) === JSON.stringify(store.data.courses ?? {})) return;
    store.data.courses = merged.courses;
    store.data.deep = merged.deep;
    write(PROGRESS, { data: store.data, savedAt, pushedAt });
    arrived = true;
  };
  let pulledAt = Date.now();

  // В аккаунт — не на каждый щелчок, а пачкой через пару секунд. Связь
  // подвела — прогресс уже лежит в браузере, отправка повторяется с паузой,
  // побольше с каждым разом, и сразу, как только сеть вернулась. Раньше
  // неудача глоталась молча, и на другом устройстве прогресс был старым.
  let pushTimer = 0;
  let retryIn = 0;
  const push = (delay = 2000) => {
    if (!user) return;
    clearTimeout(pushTimer);
    pushTimer = setTimeout(async () => {
      const sending = savedAt;
      try {
        const reply = await api("/api/noa/progress", { method: "PUT", body: { data: store.data, savedAt: sending } });
        pushedAt = Math.max(pushedAt, sending);
        adopt(reply?.data);
        write(PROGRESS, { data: store.data, savedAt, pushedAt });
        retryIn = 0;
      } catch {
        retryIn = Math.min((retryIn || 5000) * 2, 120_000);
        push(retryIn);
      }
    }, delay);
  };
  const unsent = () => Boolean(user) && savedAt > pushedAt;
  globalThis.addEventListener?.("online", () => unsent() && push(0));

  const store = {
    data,
    save() {
      savedAt = Date.now();
      write(PROGRESS, { data: store.data, savedAt, pushedAt });
      push();
    },
  };
  // Прошлый раз не всё дошло до аккаунта — дослать.
  if (localAhead || (unsent() && savedAt > remoteAt)) push(0);

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
      if (!model) throw new Error("Модель не подключена. Подключите её в настройках Ноа онлайн — значок шестерёнки в шапке.");
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
    /**
     * Убирает курс. Сам курс остаётся в этом браузере среди убранных — его
     * можно вернуть: курс собирается долго, а нажать «Убрать» с телефона
     * легко случайно.
     */
    async removeCourse(id) {
      const gone = account.find((c) => c.id === id) ?? local.find((c) => c.id === id);
      if (gone) {
        const { source, ...course } = gone;
        write(REMOVED, [course, ...read(REMOVED, []).filter((c) => c.id !== id)].slice(0, 3));
      }
      if (account.some((c) => c.id === id)) {
        await api(`/api/noa/courses/${encodeURIComponent(id)}`, { method: "DELETE" });
        account = account.filter((c) => c.id !== id);
      }
      local = local.filter((c) => c.id !== id);
      write(COURSES, local);
    },
    /** Убранные курсы этого браузера — последние три. */
    removed() {
      const here = new Set(courses().map((c) => c.id));
      return read(REMOVED, []).filter((c) => !here.has(c.id));
    },
    /** Вернуть убранный курс — в аккаунт, если вошли, иначе в браузер. */
    async restoreCourse(id) {
      const course = read(REMOVED, []).find((c) => c.id === id);
      if (!course) throw new Error("Этого курса среди убранных нет.");
      await this.addCourse(course);
      write(REMOVED, read(REMOVED, []).filter((c) => c.id !== id));
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
      // Прогресс с других устройств — раз в минуту.
      if (Date.now() - pulledAt > 60_000) {
        pulledAt = Date.now();
        adopt((await api("/api/noa/progress").catch(() => null))?.data);
      }
      const progressCame = arrived;
      arrived = false;
      try {
        const { version: now } = await api("/api/noa/courses/version");
        if (now === version) return progressCame;
        await loadAccount();
        return true;
      } catch {
        return progressCame;
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
