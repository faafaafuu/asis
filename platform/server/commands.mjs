// Команды урока для практики: после урока — набор, который стоит попробовать
// руками, каждая с тем, что она делает. Из текста урока их видно не всегда
// (в одной фразе бывает перечислено пять), поэтому набор пишет модель по
// уроку из аккаунта, один раз на урок; дальше — из памяти.

import { createHash } from "node:crypto";

import { jsonIn } from "../../src/js/web/course-builder.js";

const RULES = `Ты — наставник по практике в терминале. По уроку составь набор команд, которые стоит попробовать руками сразу после него.
- 6–12 команд в том порядке, в котором их удобно пробовать: от посмотреть к изменить.
- command — точная команда, как её набирают, без $ и без комментариев. Где нужно своё имя — понятный пример (app, nginx, web-1), а не <плейсхолдер>.
- about — что делает и что человек увидит, одной фразой до 20 слов, по-русски, без воды.
- risky: true — если команда удаляет данные, останавливает сервис или перезагружает; в about тогда сказано, чем опасна.
- Только команды по теме урока. Урок не про терминал — пустой список.
Ответ — только JSON: {"commands":[{"command":"","about":"","risky":false}]}`;

/** Набор от модели — в порядок: без мусора, длинного и повторов. */
export function shapeCommands(raw) {
  const seen = new Set();
  const out = [];
  for (const item of Array.isArray(raw?.commands) ? raw.commands : []) {
    const command = String(item?.command ?? "").replace(/^\$\s*/, "").trim();
    const about = String(item?.about ?? "").trim();
    if (!command || command.length > 160 || command.includes("\n") || !/^[\x20-\x7e]+$/.test(command) || seen.has(command)) continue;
    seen.add(command);
    out.push({ command, about: about.slice(0, 220), risky: Boolean(item?.risky) });
    if (out.length >= 14) break;
  }
  return out;
}

export function mountCommands({ route, db, Fail, readJson, chat, owner }) {
  db.exec(`
    CREATE TABLE IF NOT EXISTS noa_commands (
      user_id TEXT NOT NULL,
      course_id TEXT NOT NULL,
      topic_id TEXT NOT NULL,
      hash TEXT NOT NULL,
      body TEXT NOT NULL,
      created TEXT NOT NULL DEFAULT (datetime('now')),
      PRIMARY KEY (user_id, course_id, topic_id)
    );
  `);
  const pending = new Map();

  route("POST", /^\/api\/noa\/commands$/, async ({ req, user: who, tokenUser }) => {
    const user = owner(who ?? tokenUser);
    const { course, topic } = await readJson(req);
    const row = db.prepare("SELECT body FROM noa_courses WHERE user_id = ? AND course_id = ?").get(user.id, String(course ?? ""));
    if (!row) throw new Fail(404, "Курса нет в аккаунте — команды соберутся, когда он туда попадёт.");
    const found = (JSON.parse(row.body).topics ?? []).find((t) => t.id === String(topic ?? ""));
    if (!found) throw new Fail(404, "Такой темы в курсе нет.");
    const lesson = String(found.lesson ?? "");
    const hash = createHash("sha1").update(lesson).digest("hex").slice(0, 16);
    const cached = db.prepare("SELECT hash, body FROM noa_commands WHERE user_id = ? AND course_id = ? AND topic_id = ?").get(user.id, course, topic);
    if (cached?.hash === hash) return { commands: JSON.parse(cached.body) };

    // Два окна спросили одно и то же — модель зовём один раз.
    const key = `${user.id}/${course}/${topic}/${hash}`;
    if (!pending.has(key)) {
      pending.set(
        key,
        chat("claude-code-bridge", [{ role: "system", content: RULES }, { role: "user", content: `Тема «${found.title}».\n\nУрок:\n${lesson.slice(0, 14000)}` }], true, "sonnet")
          .then((reply) => {
            const commands = shapeCommands(jsonIn(reply));
            db.prepare(
              `INSERT INTO noa_commands (user_id, course_id, topic_id, hash, body) VALUES (?, ?, ?, ?, ?)
               ON CONFLICT(user_id, course_id, topic_id) DO UPDATE SET hash = excluded.hash, body = excluded.body, created = datetime('now')`,
            ).run(user.id, course, topic, hash, JSON.stringify(commands));
            return commands;
          })
          .finally(() => pending.delete(key)),
      );
    }
    return { commands: await pending.get(key) };
  });
}
