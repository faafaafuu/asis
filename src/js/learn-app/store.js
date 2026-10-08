// Данные приложения: курсы, практика, сборки — из программы, с запасом в
// памяти страницы, чтобы переходы между экранами не ждали каждый раз.

import { call, prefs } from "./core.js";

let overview = null;
let practice = null;
let builds = null;
const topics = new Map();

export const store = {
  /** Курсы с прогрессом (learn_overview). `fresh` — перечитать. */
  async courses(fresh = false) {
    if (!overview || fresh) overview = (await call("learn_overview").catch(() => null)) ?? overview ?? [];
    return overview;
  },

  course(id) {
    return overview?.find((c) => c.id === id) ?? null;
  },

  /** Курс, на котором остановились: последний открытый или первый. */
  currentCourse() {
    const last = prefs.get("course");
    return overview?.find((c) => c.id === last) ?? overview?.[0] ?? null;
  },

  rememberCourse(id) {
    prefs.set("course", id);
  },

  /** Тема целиком: урок, задачи, понятия, конспект (learn_topic). */
  async topic(courseId, topicId, fresh = false) {
    const key = `${courseId}/${topicId}`;
    if (!topics.has(key) || fresh) topics.set(key, await call("learn_topic", { course: courseId, topic: topicId }));
    return topics.get(key);
  },

  /** Практики тем: на каком шаге и пройдена ли (practice_progress). */
  async practice(fresh = false) {
    if (!practice || fresh) practice = (await call("practice_progress").catch(() => [])) ?? [];
    return practice;
  },

  practiceOf(courseId, topicId) {
    return practice?.find((p) => p.course === courseId && p.topic === topicId) ?? null;
  },

  /** Сборки курсов на сервере: идущая — пунктиром в «Моих курсах». */
  async builds(fresh = false) {
    if (!builds || fresh) builds = (await call("learn_builds").catch(() => [])) ?? [];
    return Array.isArray(builds) ? builds : [];
  },

  forget() {
    overview = null;
    practice = null;
    builds = null;
    topics.clear();
  },
};

/** Тема курса по id. */
export const topicCard = (course, id) => course?.topics.find((t) => t.id === id) ?? null;

/** Готовность темы в процентах — как в окне программы: урок 20, задачи 30, экзамен 50. */
export function topicPercent(topic, pass = 80) {
  const read = topic.read ? 0.2 : 0;
  const tasks = topic.tasksTotal ? (0.3 * topic.tasksDone) / topic.tasksTotal : 0.3;
  const exam = 0.5 * Math.min(1, (topic.examBest ?? 0) / pass);
  return Math.round((read + tasks + exam) * 100);
}

export const isDone = (topic) => topic.status === "done";

/** Тема, на которой человек сейчас: отмеченная курсом или первая непройденная. */
export function currentTopic(course) {
  if (!course) return null;
  return topicCard(course, course.current) ?? course.topics.find((t) => !isDone(t)) ?? course.topics[0] ?? null;
}

export function greeting(now = new Date()) {
  const hour = now.getHours();
  if (hour < 5) return "Доброй ночи";
  if (hour < 12) return "Доброе утро";
  if (hour < 18) return "Добрый день";
  return "Добрый вечер";
}
