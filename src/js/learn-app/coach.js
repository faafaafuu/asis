// Тренер — модель по указаниям экрана («Наизусть», «Собеседование»). Одна
// команда learn_coach и в программе, и в браузере; здесь — разбор ответа.

import { call } from "./core.js";

/** Спросить модель. `thread` — прошлые обмены `{q, a}`; `long` — длинный ответ. */
export function coach(rules, said, { thread = [], long = false } = {}) {
  return call("learn_coach", { rules, said, thread, long }, { slow: true });
}

/** JSON из ответа модели: она охотно пишет пояснения вокруг. */
export function parseJson(text) {
  const raw = String(text ?? "");
  const starts = [raw.indexOf("{"), raw.indexOf("[")].filter((at) => at >= 0);
  if (!starts.length) throw new Error("Модель ответила не по форме — попробуйте ещё раз.");
  const from = Math.min(...starts);
  const to = Math.max(raw.lastIndexOf("}"), raw.lastIndexOf("]"));
  try {
    return JSON.parse(raw.slice(from, to + 1));
  } catch {
    throw new Error("Модель ответила не по форме — попробуйте ещё раз.");
  }
}

/** JSON-ответ модели — с одним повтором, если форма не та. */
export async function coachJson(rules, said, options) {
  try {
    return parseJson(await coach(rules, said, options));
  } catch (err) {
    if (!/не по форме/.test(String(err))) throw err;
    return parseJson(await coach(`${rules}\nСТРОГО: только JSON, без слов вокруг.`, said, options));
  }
}

/** Проверка ответа «по памяти»: насколько он передаёт определение. */
export async function checkRecall(term, definition, answer) {
  const rules =
    "Ты — репетитор: проверяешь, выучил ли ученик определение наизусть. Сравни ответ с эталоном по смыслу и ключевым словам, " +
    "мелкие переформулировки допустимы, пропуск сути — нет. Ответь только JSON: " +
    '{"score": 0-100, "missing": ["чего не хватило, кратко"], "say": "одна короткая фраза ученику"}';
  return coachJson(rules, `Понятие: ${term}\nЭталон: ${definition}\nОтвет ученика: ${answer}`);
}
