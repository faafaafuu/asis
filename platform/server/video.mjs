// Ролики к урокам: короткое видео-схема по разделу или по всей теме.
//
// Сценарий и код ролика пишет модель через мост — из готовых деталей
// noah_kit (platform/video), по образцу example_scene.py. Рендерит не сайт:
// у службы сайта нет и не должно быть доступа к Docker. Сайт кладёт задание
// в папку, служба noah-video (worker.py) озвучивает, рендерит Manim в
// контейнере без сети и склеивает. Упал рендер — журнал уходит модели, она
// присылает исправленный код; до трёх попыток.

import { createReadStream, existsSync, readFileSync, statSync } from "node:fs";
import { mkdir, readFile, rm, writeFile } from "node:fs/promises";
import { randomUUID } from "node:crypto";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const HERE = dirname(fileURLToPath(import.meta.url));
const ROOT = process.env.NOAH_VIDEO_DIR ?? "/opt/noah-video";
const JOBS = join(ROOT, "jobs");
const EXAMPLE = join(HERE, "..", "video", "example_scene.py");
const ICONS = join(HERE, "..", "..", "src", "styles", "noah", "icons.svg");
const MAX_MATERIAL = 7000;
const ATTEMPTS = 3;
const JOB_LIMIT = 25 * 60_000;

const STAGES = {
  script: "Пишу сценарий",
  fix: "Исправляю код ролика",
  queued: "Жду очереди",
  voice: "Озвучиваю",
  render: "Рисую ролик",
  mux: "Склеиваю звук",
};

/** Иконки, которые есть в наборе, — модели перечнем. */
function iconNames() {
  try {
    return [...readFileSync(ICONS, "utf8").matchAll(/<symbol id="i-([a-z0-9-]+)"/g)].map((m) => m[1]).join(", ");
  } catch {
    return "cpu, cloud, log, gauge, chart, check, refresh, key, globe, link, lock, window, keyboard, flame, alert, user";
  }
}

export const rules = () => `Ты делаешь короткий обучающий ролик-схему: Manim на готовых деталях noah_kit, тема оформления «Неон».
Ролик объясняет главное из материала схемой: блоки, связи, данные, которые бегут по стрелкам, шаги по номерам.

Ответ — строго в таком виде, без текста вокруг:
TITLE: название ролика, 2–6 слов
SCENES:
1. фраза озвучки сцены 0
2. фраза озвучки сцены 1
…
CODE:
\`\`\`python
код ролика
\`\`\`

Озвучка:
- 4–7 сцен. Каждая — одна-две фразы, до 28 слов: живо и просто, как объясняет друг.
- Числа и порты — словами: «восемь тысяч восемьдесят». Ни кода, ни путей, ни команд в озвучке.
- Сцена 0 — заставка (self.intro), последняя — итог (self.points и self.show_points).

Код:
- Только \`from noah_kit import *\`; класс \`class Video(NoahScene)\` с методом construct. Других import нет,
  файлов, сети, eval, exec — нет.
- Сцена i кончается self.hold(i); hold по порядку, сколько фраз — столько hold.
- Время сцены — её озвучка: держи анимации сцены в 3–5 секунд, остальное доберёт hold.

Детали noah_kit (всё уже импортировано):
- card(название, подпись, иконка, цвет, тип, w=3.0, h=1.8) — блок с шапкой типа («контейнер», «сайдкар»,
  «внешний сервис», «база данных», «узел», «пользователь»…), иконкой и свечением. Части: card[1] — рамка
  (к ней цеплять стрелки: card[1].get_right()), card[4] — надписи.
- zone(заголовок, w, h, цвет) — область: под, узел, кластер, сеть. Внутри — блоки.
- wire(a, b, подпись, цвет, step=номер) — стрелка от точки a к b, подпись 1–2 слова, номер шага кружком.
  Появление: self.go(GrowArrow(w[1]), FadeIn(w[0]), FadeIn(w[2:]), rt=0.6).
- self.send(wire, ["пакет", "пакет"], цвет, reverse=False) — данные идут по стрелке; надпись пакета до 10 знаков,
  2–3 пакета на стрелку. reverse=True — против стрелки (кто-то забирает).
- self.say("мысль сцены") — субтитр внизу, до 50 знаков: отдаёт анимации, их передают в self.go(*self.say(…)).
- self.focus(блок, блок, …) — выделить, остальное в тень; self.focus() — снять. Тоже отдаёт анимации.
- self.intro(метка, название, подзаголовок) — заставка; self.clear() — убрать всё перед следующей сценой.
- terminal(заголовок, w=7.2, h=3.4) и self.type(терминал, ["$ команда", "вывод"]) — окно терминала и набор строк.
- self.points([(иконка, цвет, текст), …]) — итог списком; .move_to(…) и self.show_points(группа).
- self.go(*анимации, rt=секунды) — играть анимации (всегда через go, не self.play).
- badge(n), packet(текст), icon(имя, цвет, размер), glow(mob, цвет), fit(mob, ширина) — по нужде.
- Цвета: CY — главное и приложение, MG — вспомогательное и сайдкары, OK — внешнее и успех, GOLD — метрики и
  внимание, TX — текст, MU — приглушённый текст. Шрифты: HEAD (заголовки), MONO (остальное).
- Иконки: ${iconNames()}.

Раскладка: кадр 14.2×8, центр (0, 0). Карточки 3.0×1.8; в ряд не больше трёх, центры по x около −5.15, 0.15, 5.45
(между карточками ≥ 2.3 — там идут пакеты); ряды по y около 1.15 и −1.55. Ниже y = −2.9 ничего не ставь — там
субтитры. Надписи не длиннее карточки.

Образец целиком — так по устройству, но про свою тему: свои блоки, связи и логика, не копируй сайдкар.
\`\`\`python
${readFileSync(EXAMPLE, "utf8")}
\`\`\``;

/** Разобрать ответ модели: название, фразы, код. */
export function parseVideo(text) {
  const title = /TITLE:\s*(.+)/.exec(text)?.[1]?.trim() ?? "";
  const scenesBlock = /SCENES:\s*([\s\S]*?)\n\s*CODE:/.exec(text)?.[1] ?? "";
  const scenes = scenesBlock
    .split("\n")
    .map((line) => line.replace(/^\s*\d+[.)]\s*/, "").trim())
    .filter(Boolean);
  const code = /```(?:python)?\s*\n([\s\S]*?)```/.exec(text.slice(text.indexOf("CODE:")))?.[1] ?? "";
  return { title, scenes, code: code.trim() + "\n" };
}

/**
 * Код без содержимого обычных строк: подписи на схеме («open() открывает
 * файл», «os и sys») — не код и не должны валить проверку. f-строки
 * остаются: выражения в них выполняются.
 */
function withoutText(code) {
  const literal = /(?<!\w)([rRbBuUfF]{0,2})("""[\s\S]*?"""|'''[\s\S]*?'''|"(?:[^"\\\n]|\\.)*"|'(?:[^'\\\n]|\\.)*')/g;
  return code.replace(literal, (all, prefix) => (/f/i.test(prefix) ? all : `${prefix}""`));
}

/** Код из ответа — только на деталях набора. Отдаёт, что не так, или "". */
export function checkCode(code, scenes) {
  if (!/class\s+Video\s*\(\s*NoahScene\s*\)/.test(code)) return "нет класса Video(NoahScene)";
  const imports = code.split("\n").filter((line) => /^\s*(import|from)\s/.test(line));
  if (imports.some((line) => line.trim() !== "from noah_kit import *")) return "лишний import — разрешён только from noah_kit import *";
  const banned = /\b(open|exec|eval|compile|__import__|getattr|setattr|globals|locals|input)\s*\(|\b(os|sys|subprocess|socket|shutil|pathlib|urllib|requests|builtins)\b|__\w+__/;
  const found = banned.exec(withoutText(code).replace(/def\s+__init__|__name__/g, ""));
  if (found) return `запрещено: ${found[0]}`;
  const holds = [...code.matchAll(/self\.hold\(\s*(\d+)\s*\)/g)].map((m) => Number(m[1]));
  if (holds.length !== scenes.length) return `self.hold — ${holds.length}, а фраз озвучки — ${scenes.length}: должно совпадать`;
  if (holds.some((n, i) => n !== i)) return "self.hold должны идти по порядку: 0, 1, 2…";
  return "";
}

export function mountVideo({ route, db, Fail, readJson, chat, owner }) {
  db.exec(`
    CREATE TABLE IF NOT EXISTS noa_videos (
      id TEXT PRIMARY KEY,
      user_id TEXT NOT NULL,
      course_id TEXT NOT NULL,
      topic_id TEXT NOT NULL,
      section INTEGER,
      title TEXT NOT NULL DEFAULT '',
      status TEXT NOT NULL,
      stage TEXT NOT NULL DEFAULT 'script',
      message TEXT NOT NULL DEFAULT '',
      material TEXT NOT NULL DEFAULT '',
      created TEXT NOT NULL DEFAULT (datetime('now')),
      updated TEXT NOT NULL DEFAULT (datetime('now'))
    );
    CREATE INDEX IF NOT EXISTS noa_videos_topic ON noa_videos(user_id, course_id, topic_id);
  `);

  const row = (r) =>
    r && {
      id: r.id,
      course: r.course_id,
      topic: r.topic_id,
      section: r.section,
      title: r.title,
      status: r.status,
      stage: r.stage,
      stageText: STAGES[r.stage] ?? "",
      message: r.message,
      created: r.created,
      updated: r.updated,
    };

  const update = (id, fields) => {
    const keys = Object.keys(fields);
    db.prepare(`UPDATE noa_videos SET ${keys.map((k) => `${k} = ?`).join(", ")}, updated = datetime('now') WHERE id = ?`).run(
      ...keys.map((k) => fields[k]),
      id,
    );
  };

  const running = new Set();
  const sleep = (ms) => new Promise((done) => setTimeout(done, ms));

  async function write(job, name, value) {
    await writeFile(join(job, name), typeof value === "string" ? value : JSON.stringify(value), { mode: 0o666 });
  }

  async function ask(messages) {
    return chat("claude-code-bridge", messages, true, "sonnet");
  }

  /** Сценарий и код — с проверкой; не прошёл — модель исправляет. */
  async function script(id, material, messages) {
    let reply = await ask(messages);
    let parsed = parseVideo(reply);
    for (let attempt = 0; attempt < ATTEMPTS; attempt++) {
      const problem = parsed.scenes.length < 3 ? "нет фраз озвучки (SCENES)" : checkCode(parsed.code, parsed.scenes);
      if (!problem) return { parsed, messages: [...messages, { role: "assistant", content: reply }] };
      console.log(`ролик ${id.slice(0, 8)}: код не прошёл проверку — ${problem}`);
      update(id, { stage: "fix" });
      const fix = [...messages, { role: "assistant", content: reply }, { role: "user", content: `Код не прошёл проверку: ${problem}. Пришли ответ заново целиком в том же виде.` }];
      reply = await ask(fix);
      parsed = parseVideo(reply);
    }
    throw new Error("Модель не написала годный код ролика.");
  }

  async function run(job) {
    if (running.has(job.id)) return;
    running.add(job.id);
    const dir = join(JOBS, job.id);
    const deadline = Date.now() + JOB_LIMIT;
    try {
      await mkdir(dir, { recursive: true, mode: 0o777 }).catch((err) => {
        console.error("ролик: папка задания:", err.message);
        throw new Error("Сборка роликов на сервере сейчас недоступна — попробуйте позже.");
      });
      let messages = [
        { role: "system", content: rules() },
        { role: "user", content: job.material },
      ];
      let scenes;
      if (existsSync(join(dir, "scene.py")) && existsSync(join(dir, "scenes.json"))) {
        // Сервер перезапустили посреди рендера — продолжаем с готового кода.
        scenes = JSON.parse(await readFile(join(dir, "scenes.json"), "utf8"));
      } else {
        update(job.id, { stage: "script" });
        const done = await script(job.id, job.material, messages);
        messages = done.messages;
        scenes = done.parsed.scenes;
        update(job.id, { title: done.parsed.title.slice(0, 120) });
        await write(dir, "scenes.json", scenes);
        await write(dir, "scene.py", done.parsed.code);
        await write(dir, "ready", "");
        update(job.id, { stage: "queued" });
      }
      for (let attempt = 1; ; ) {
        if (Date.now() > deadline) throw new Error("Ролик собирался слишком долго — попробуйте ещё раз.");
        await sleep(3000);
        try {
          const status = JSON.parse(await readFile(join(dir, "status.json"), "utf8"));
          if (STAGES[status.stage]) update(job.id, { stage: status.stage });
        } catch {
          /* служба ещё не взяла задание */
        }
        if (existsSync(join(dir, "done.json"))) {
          update(job.id, { status: "done", stage: "done", message: "" });
          return;
        }
        if (!existsSync(join(dir, "error.json"))) continue;
        const { log } = JSON.parse(await readFile(join(dir, "error.json"), "utf8"));
        if (attempt >= ATTEMPTS) throw new Error("Ролик не отрисовался — попробуйте собрать ещё раз.");
        attempt += 1;
        update(job.id, { stage: "fix" });
        const reply = await ask([
          ...messages,
          {
            role: "user",
            content: `Рендер упал. Конец журнала:\n\`\`\`\n${String(log).slice(-3500)}\n\`\`\`\nИсправь код. Пришли ответ целиком в том же виде, фразы SCENES — те же.`,
          },
        ]);
        const fixed = parseVideo(reply);
        const problem = checkCode(fixed.code, scenes);
        if (problem) continue;
        messages = [...messages, { role: "assistant", content: reply }];
        await rm(join(dir, "error.json"), { force: true });
        await write(dir, "scene.py", fixed.code);
        await write(dir, "ready", "");
        update(job.id, { stage: "queued" });
      }
    } catch (err) {
      console.error("ролик:", err?.message ?? err);
      update(job.id, { status: "failed", message: String(err?.message ?? err).slice(0, 300) });
    } finally {
      running.delete(job.id);
    }
  }

  // Сервер перезапустили — недоделанные ролики дорабатываются.
  for (const job of db.prepare("SELECT * FROM noa_videos WHERE status = 'running'").all()) run(job);

  route("POST", /^\/api\/noa\/videos$/, async ({ req, user: who, tokenUser }) => {
    const user = owner(who ?? tokenUser);
    const body = await readJson(req, 64_000);
    const course = String(body.course ?? "").slice(0, 120);
    const topic = String(body.topic ?? "").slice(0, 120);
    const section = Number.isInteger(body.section) ? body.section : null;
    const material = String(body.material ?? "").trim().slice(0, MAX_MATERIAL);
    if (!course || !topic || material.length < 40) throw new Fail(400, "Не из чего делать ролик: нет материала урока.");
    const same = db
      .prepare("SELECT * FROM noa_videos WHERE user_id = ? AND course_id = ? AND topic_id = ? AND section IS ? ORDER BY created DESC LIMIT 1")
      .get(user.id, course, topic, section);
    if (same && (same.status === "running" || (same.status === "done" && !body.again))) return row(same);
    const busy = db.prepare("SELECT COUNT(*) AS n FROM noa_videos WHERE user_id = ? AND status = 'running'").get(user.id).n;
    if (busy >= 3) throw new Fail(429, "Уже собираются три ролика — дождитесь их.");
    const id = randomUUID();
    db.prepare("INSERT INTO noa_videos (id, user_id, course_id, topic_id, section, status, material) VALUES (?, ?, ?, ?, ?, 'running', ?)").run(
      id,
      user.id,
      course,
      topic,
      section,
      material,
    );
    const job = db.prepare("SELECT * FROM noa_videos WHERE id = ?").get(id);
    run(job);
    return row(job);
  });

  route("GET", /^\/api\/noa\/videos$/, ({ url, user: who, tokenUser }) => {
    const user = owner(who ?? tokenUser);
    const rows = db
      .prepare("SELECT * FROM noa_videos WHERE user_id = ? AND course_id = ? AND topic_id = ? ORDER BY created DESC")
      .all(user.id, url.searchParams.get("course") ?? "", url.searchParams.get("topic") ?? "");
    // По одному на раздел — последний.
    const seen = new Set();
    return {
      videos: rows
        .filter((r) => {
          const key = String(r.section);
          if (seen.has(key)) return false;
          seen.add(key);
          return true;
        })
        .map(row),
    };
  });

  route("GET", /^\/api\/noa\/videos\/([0-9a-f-]{36})$/, ({ match, user: who, tokenUser }) => {
    const user = owner(who ?? tokenUser);
    const found = db.prepare("SELECT * FROM noa_videos WHERE id = ? AND user_id = ?").get(match[1], user.id);
    if (!found) throw new Fail(404, "Такого ролика нет.");
    return row(found);
  });

  // Файл ролика — с докачкой по Range: канал из России рвёт длинные ответы,
  // программа забирает ролик кусками, браузер — как обычно.
  route("GET", /^\/api\/noa\/videos\/([0-9a-f-]{36})\/(video\.mp4|poster\.jpg)$/, ({ req, res, match, user: who, tokenUser }) => {
    const user = owner(who ?? tokenUser);
    const found = db.prepare("SELECT id FROM noa_videos WHERE id = ? AND user_id = ?").get(match[1], user.id);
    if (!found) throw new Fail(404, "Такого ролика нет.");
    const file = join(JOBS, found.id, match[2]);
    if (!existsSync(file)) throw new Fail(404, "Ролик ещё не готов.");
    const size = statSync(file).size;
    const type = match[2].endsWith(".mp4") ? "video/mp4" : "image/jpeg";
    const range = /^bytes=(\d*)-(\d*)$/.exec(String(req.headers.range ?? ""));
    const head = {
      "Content-Type": type,
      "Accept-Ranges": "bytes",
      // Ролик под этим адресом не меняется: пересобранный — новый id.
      "Cache-Control": "private, max-age=31536000, immutable",
      "X-Content-Type-Options": "nosniff",
    };
    if (range && (range[1] || range[2])) {
      const start = range[1] ? Number(range[1]) : Math.max(0, size - Number(range[2]));
      const end = range[1] && range[2] ? Math.min(Number(range[2]), size - 1) : size - 1;
      if (start >= size || end < start) {
        res.writeHead(416, { "Content-Range": `bytes */${size}` });
        return res.end();
      }
      res.writeHead(206, { ...head, "Content-Range": `bytes ${start}-${end}/${size}`, "Content-Length": end - start + 1 });
      createReadStream(file, { start, end }).pipe(res);
      return;
    }
    res.writeHead(200, { ...head, "Content-Length": size });
    createReadStream(file).pipe(res);
  });
}
