// Аудиодрамы к урокам: материал урока — живая сцена на три голоса.
//
// Слушатель запоминает через историю, а не заучивает: джун паникует и
// задаёт вопросы слушателя, Лена спокойно объясняет, Палыч ворчит и даёт
// заповеди. Серия — инцидент со ставкой, ошибка джуна, заповедь, развязка;
// после — вопросы, привязанные к сценам, и три вопроса на повтор через день.
//
// Сценарий пишет модель через мост — серия за серией, каждая проверяется
// (голоса, длина реплик, произносимость, вопросы со ссылками на сцены) и при
// ошибках возвращается модели на исправление. Озвучивает служба noah-video
// (worker.py): каждый голос свой, подача — темпом, высотой и громкостью;
// выходит MP3 и разметка реплик по времени — окно подсвечивает, кто говорит,
// и по вопросу возвращает к нужной сцене.

import { createReadStream, existsSync, statSync } from "node:fs";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import { randomUUID } from "node:crypto";
import { join } from "node:path";

const ROOT = process.env.NOAH_VIDEO_DIR ?? "/opt/noah-video";
const JOBS = join(ROOT, "jobs");
const MAX_MATERIAL = 9000;
const ATTEMPTS = 3;
const EPISODE_LIMIT = 15 * 60_000;
const MAX_EPISODES = 6;

export const SPEAKERS = ["junior", "lena", "palych", "narrator"];
export const DELIVERY = ["быстро", "на нервах", "шёпотом", "спокойно", "с иронией", "хрипло", "зловеще", "радостно", "растерянно", "твёрдо"];
const QUIZ_TYPES = ["recall", "scenario", "compare"];

const STAGES = {
  script: "Пишу сценарий",
  fix: "Правлю сценарий",
  queued: "Жду очереди",
  voice: "Озвучиваю",
  mix: "Свожу звук",
};

/**
 * Как инженеры произносят частые слова — модель путалась сама: kubectl у неё
 * звучал «кубконтрол». Новое слово, которого нет здесь, она выносит в new_terms.
 */
export const PRONOUNCE = [
  ["kubectl", "кубкатл"],
  ["kubelet", "кублет"],
  ["etcd", "и-ти-си-ди"],
  ["Ingress", "ингресс"],
  ["Helm", "хелм"],
  ["Docker", "докер"],
  ["YAML", "ямл"],
  ["JSON", "джейсон"],
  ["sudo", "судо"],
  ["systemd", "систем-ди"],
  ["cron", "крон"],
  ["PostgreSQL", "постгрес"],
  ["Redis", "редис"],
  ["Linux", "линукс"],
  ["HTTP", "эйч-ти-ти-пи"],
  ["HTTPS", "эйч-ти-ти-пи-эс"],
  ["TLS", "ти-эл-эс"],
  ["API", "эй-пи-ай"],
  ["CPU", "си-пи-ю"],
  ["RAM", "рам"],
  ["SIGTERM", "сиг-терм"],
  ["SIGKILL", "сиг-килл"],
  ["Prometheus", "прометеус"],
  ["Grafana", "графана"],
  ["Terraform", "терраформ"],
  ["Ansible", "ансибл"],
  ["Git", "гит"],
  ["RPC", "ар-пи-си"],
  ["geth", "гет"],
]
  .map(([term, spoken]) => `${term} — «${spoken}»`)
  .join(", ");

/** Указания модели: одна серия за ответ. */
export function rules({ level, junior, minutes, scope, total }) {
  const words = minutes * 125;
  return `Ты — сценарист обучающих аудиодрам на технические темы. Превращаешь материал урока в живую сцену, которую слушают на разные голоса. Цель — чтобы слушатель запомнил через историю, а не заучивал.

ПЕРСОНАЖИ — постоянные, манеру речи не менять:
- junior — ${junior}, джун: паникует, задаёт вопросы, которые задал бы слушатель. Говорит быстро, срывается.
- lena — Лена: спокойная, с иронией, объясняет чётко.
- palych — Палыч: старый сисадмин, ворчит, хрипит, даёт правила-«заповеди».
- narrator — рассказчик: только короткие ремарки обстановки в начале сцены («Ночь. Офис пуст. Звонит телефон.»).
Уровень слушателя: ${level === "middle" ? "middle — основы знает, нужны тонкости, компромиссы и разбор отказов" : "junior — объяснять с нуля, без жаргона без пояснения"}.

ДРАМАТУРГИЯ
1. Завязка — настоящий инцидент со ставкой: сайт лежит, звонят клиенты, ночью дедлайн.
2. Объяснение — через действия и ошибки героев, не лекцией. Не больше двух реплик подряд без вопроса, ошибки или действия.
3. В каждой серии: одна ошибка ${junior}, одна заповедь Палыча (короткой фразой, которую хочется запомнить), развязка.
4. Ключевой термин серии звучит 2–3 раза в разных ситуациях.
4а. Проблему решают вслух, как на настоящем дежурстве, шаг за шагом: догадка — «введи вот это» и что эта команда делает — что она показала — вывод — следующий шаг. Не меньше ${Math.max(3, minutes - 1)} таких шагов за серию; хотя бы один — ложный след: проверили, не то, и сказано почему. Слушатель должен уметь повторить этот разбор сам.
${scope === "section" ? `5. Это сериал из ${total} серий со сквозным сюжетом. Серия со второй начинается с «В прошлой серии…» — пересказ прошлого материала внутри истории — и каждая кончается крючком к следующей.` : "5. Одна серия — законченная история."}

ТОЧНОСТЬ — проверь до ответа
- Каждое техническое утверждение верно. Не уверен — не пиши.
- Аналогия — только если каждой части оригинала соответствует одна часть аналогии, она из знакомой слушателю области и сказано, где она ломается. Иначе объясняй напрямую.
- Сначала суть, потом образ. Термин вводится внутри диалога.

ОЗВУЧКА
- delivery — одно из: ${DELIVERY.join(", ")}.
- spoken — текст, полностью готовый к произнесению русским синтезатором речи: ни одной латинской буквы, цифры или знака кода.
  • Латиницу и аббревиатуры — русскими буквами, как говорят инженеры: TCP — «ти-си-пи», SYN — «син», ACK — «эк», DNS — «ди-эн-эс», SSH — «эс-эс-аш», nginx — «энджин-икс», Kubernetes — «кубернетис», pod — «под».
  • Устоявшееся произношение — только так: ${PRONOUNCE}.
  • Команды не зачитывай символами — пересказывай смысл: «проверяю порт четыреста сорок три утилитой эн-си». Если сама команда — тема урока, читай по словам.
  • Числа, порты и адреса — словами: 203.0.113.5 — «двести три точка ноль точка сто тринадцать точка пять».
  • Не уверен в произношении термина — добавь его в new_terms.
- display — то же с латиницей, кодом и цифрами, как это пишут.
- command — у реплики, где героя просят ввести команду или он её вводит: точная команда для терминала, как её набирают. В spoken её не зачитывать символами — сказать, что она делает: «введи кубкатл гет подс — увидишь, какие поды падают».
- output — у реплики, где смотрят результат: 1–4 строки того, что показал терминал, правдоподобно и сокращённо. В spoken — пересказ главного: «рестартов одиннадцать, статус краш-луп-бэк-офф».
- Реплика — до 25 слов. Ремарка рассказчика — до 20.
- Серия — около ${minutes} минут звучания: примерно ${words} слов в репликах, ${Math.round(minutes * 11)}–${Math.round(minutes * 14)} реплик, 3–6 сцен.

ВОПРОСЫ
- quiz — 5–8 вопросов после серии, все три типа: recall (вспомнить), scenario (что делать), compare (чем отличается).
- Каждый вопрос привязан к сцене и реплике (ref) — слушатель вспоминает момент истории. options — 3–4 варианта, answer — один из них дословно. explain — коротко, почему так, со ссылкой на момент истории.
- delayed_quiz — ещё ровно 3 вопроса на повтор через день: без подсказок из сцены, ref не нужен.

ВЫХОД — только JSON, без текста вокруг, одна серия:
{"title":"название сериала","new_terms":[{"term":"","spoken":""}],
 "episode":{"title":"","recap":"","hook":"",
  "scenes":[{"id":"s1","setting":"ремарка рассказчика, готовая к произнесению","lines":[{"id":"l1","speaker":"junior","delivery":"на нервах","spoken":"","display":"","command":"","output":""}]}],
  "quiz":[{"type":"recall","q":"","options":["",""],"answer":"","explain":"","ref":{"scene":"s1","line":"l1"}}],
  "delayed_quiz":[{"type":"recall","q":"","options":["",""],"answer":"","explain":""}]}}
id сцен — s1, s2…; id реплик — l1, l2… сквозные по серии. recap и hook — тоже готовы к произнесению; recap пустой в первой серии, hook — пустой только в последней.`;
}

/** JSON из ответа модели: без ```json и текста вокруг. */
export function parseEpisode(text) {
  const start = text.indexOf("{");
  const end = text.lastIndexOf("}");
  if (start < 0 || end <= start) return null;
  try {
    return JSON.parse(text.slice(start, end + 1));
  } catch {
    return null;
  }
}

const words = (text) => String(text ?? "").trim().split(/\s+/).filter(Boolean).length;
// Синтезатор не прочтёт латиницу, цифры и знаки кода — их быть не должно.
const unspeakable = (text) => /[A-Za-z0-9_$#@<>{}[\]|\\/=*`~^]/.test(String(text ?? ""));

/**
 * Проверка серии. Отдаёт список того, что не так, — его возвращают модели.
 * `n` — номер серии, `total` — сколько всего, `minutes` — длина.
 */
export function checkEpisode(reply, { n = 1, total = 1, minutes = 5 } = {}) {
  const problems = [];
  const ep = reply?.episode;
  if (!ep || !Array.isArray(ep.scenes)) return ["нет episode.scenes — пришли JSON в указанном виде"];
  if (!String(ep.title ?? "").trim()) problems.push("у серии нет title");
  if (ep.scenes.length < 2) problems.push("сцен меньше двух");
  const lines = new Map();
  let spokenWords = 0;
  let commands = 0;
  for (const scene of ep.scenes) {
    if (!scene?.id || !Array.isArray(scene.lines) || !scene.lines.length) {
      problems.push(`сцена ${scene?.id ?? "?"}: нет id или реплик`);
      continue;
    }
    if (unspeakable(scene.setting)) problems.push(`сцена ${scene.id}: в ремарке латиница или цифры — перепиши словами`);
    for (const line of scene.lines) {
      const where = `сцена ${scene.id}, реплика ${line?.id ?? "?"}`;
      if (!line?.id) problems.push(`${where}: нет id`);
      lines.set(`${scene.id}/${line?.id}`, true);
      if (!SPEAKERS.includes(line?.speaker)) problems.push(`${where}: speaker «${line?.speaker}» — только ${SPEAKERS.join(", ")}`);
      if (!DELIVERY.includes(line?.delivery)) problems.push(`${where}: delivery «${line?.delivery}» не из набора`);
      if (!String(line?.spoken ?? "").trim()) problems.push(`${where}: пустой spoken`);
      if (unspeakable(line?.spoken)) problems.push(`${where}: в spoken латиница, цифры или знаки — «${String(line.spoken).slice(0, 60)}»`);
      const count = words(line?.spoken);
      spokenWords += count;
      if (count > 28) problems.push(`${where}: ${count} слов — реплика до 25`);
      if (String(line?.command ?? "").trim()) commands += 1;
    }
  }
  // Разбор вслух — с командами: «введи вот это, оно покажет то-то».
  const needed = Math.max(3, minutes - 1);
  if (commands < needed) problems.push(`команд в разборе ${commands} — нужно не меньше ${needed}: герои рассуждают вслух и вводят команды (поле command), смотрят вывод (output)`);
  const target = minutes * 125;
  if (spokenWords < target * 0.55) problems.push(`в репликах ${spokenWords} слов — для ${minutes} минут нужно около ${target}`);
  if (spokenWords > target * 1.6) problems.push(`в репликах ${spokenWords} слов — для ${minutes} минут многовато, нужно около ${target}`);
  if (n > 1 && !String(ep.recap ?? "").trim()) problems.push("нет recap — серия со второй начинается с «В прошлой серии…»");
  if (n < total && !String(ep.hook ?? "").trim()) problems.push("нет hook — крючка к следующей серии");
  if (unspeakable(ep.recap) || unspeakable(ep.hook)) problems.push("в recap или hook латиница или цифры");

  const quiz = Array.isArray(ep.quiz) ? ep.quiz : [];
  if (quiz.length < 5 || quiz.length > 8) problems.push(`вопросов ${quiz.length} — нужно 5–8`);
  for (const type of QUIZ_TYPES) {
    if (!quiz.some((q) => q?.type === type)) problems.push(`нет вопроса типа ${type}`);
  }
  quiz.forEach((q, i) => {
    if (!QUIZ_TYPES.includes(q?.type)) problems.push(`вопрос ${i + 1}: type «${q?.type}»`);
    if (!String(q?.q ?? "").trim() || !String(q?.answer ?? "").trim()) problems.push(`вопрос ${i + 1}: нет q или answer`);
    if (Array.isArray(q?.options) && q.options.length && !q.options.includes(q.answer)) problems.push(`вопрос ${i + 1}: answer не совпадает ни с одним из options`);
    if (!lines.has(`${q?.ref?.scene}/${q?.ref?.line}`)) problems.push(`вопрос ${i + 1}: ref ${q?.ref?.scene}/${q?.ref?.line} — нет такой реплики`);
  });
  const delayed = Array.isArray(ep.delayed_quiz) ? ep.delayed_quiz : [];
  if (delayed.length !== 3) problems.push(`delayed_quiz — ${delayed.length}, нужно ровно 3`);
  return problems;
}

/** Что о прошлых сериях знать следующей: сюжет, термины, крючок. */
function previously(episodes) {
  return episodes
    .map((ep) => {
      const settings = ep.scenes.map((s) => s.setting).filter(Boolean).join(" ");
      return `Серия ${ep.n} «${ep.title}». ${settings} Крючок: ${ep.hook || "—"}`;
    })
    .join("\n");
}

export function mountDrama({ route, db, Fail, readJson, chat, owner }) {
  db.exec(`
    CREATE TABLE IF NOT EXISTS noa_dramas (
      id TEXT PRIMARY KEY,
      user_id TEXT NOT NULL,
      course_id TEXT NOT NULL,
      topic_id TEXT NOT NULL,
      scope TEXT NOT NULL DEFAULT 'lesson',
      title TEXT NOT NULL DEFAULT '',
      status TEXT NOT NULL,
      stage TEXT NOT NULL DEFAULT 'script',
      message TEXT NOT NULL DEFAULT '',
      params TEXT NOT NULL DEFAULT '{}',
      material TEXT NOT NULL DEFAULT '',
      total INTEGER NOT NULL DEFAULT 1,
      ready INTEGER NOT NULL DEFAULT 0,
      created TEXT NOT NULL DEFAULT (datetime('now')),
      updated TEXT NOT NULL DEFAULT (datetime('now'))
    );
    CREATE INDEX IF NOT EXISTS noa_dramas_topic ON noa_dramas(user_id, course_id, topic_id);
  `);

  const row = (r) =>
    r && {
      id: r.id,
      course: r.course_id,
      topic: r.topic_id,
      scope: r.scope,
      title: r.title,
      status: r.status,
      stage: r.stage,
      stageText: STAGES[r.stage] ?? "",
      message: r.message,
      params: JSON.parse(r.params || "{}"),
      total: r.total,
      ready: r.ready,
      created: r.created,
      updated: r.updated,
    };

  const update = (id, fields) => {
    const keys = Object.keys(fields);
    db.prepare(`UPDATE noa_dramas SET ${keys.map((k) => `${k} = ?`).join(", ")}, updated = datetime('now') WHERE id = ?`).run(
      ...keys.map((k) => fields[k]),
      id,
    );
  };

  const running = new Set();
  const sleep = (ms) => new Promise((done) => setTimeout(done, ms));
  const write = (dir, name, value) =>
    writeFile(join(dir, name), typeof value === "string" ? value : JSON.stringify(value), { mode: 0o666 });

  /** Одна серия: ответ модели, проверка, до трёх исправлений. */
  async function episode(id, messages, check) {
    let reply = await chat("claude-code-bridge", messages, true, "sonnet");
    for (let attempt = 0; ; attempt++) {
      const parsed = parseEpisode(reply);
      const problems = parsed ? checkEpisode(parsed, check) : ["ответ — не JSON; пришли только JSON в указанном виде"];
      if (!problems.length) return parsed;
      console.log(`драма ${id.slice(0, 8)}: серия ${check.n} не прошла проверку — ${problems.slice(0, 3).join("; ")}`);
      if (attempt >= ATTEMPTS) throw new Error("Модель не написала годный сценарий — попробуйте ещё раз.");
      update(id, { stage: "fix" });
      reply = await chat(
        "claude-code-bridge",
        [
          ...messages,
          { role: "assistant", content: reply },
          { role: "user", content: `Сценарий не прошёл проверку:\n- ${problems.slice(0, 25).join("\n- ")}\nИсправь и пришли серию заново целиком, только JSON.` },
        ],
        true,
        "sonnet",
      );
    }
  }

  async function run(job) {
    if (running.has(job.id)) return;
    running.add(job.id);
    const dir = join(JOBS, job.id);
    const params = JSON.parse(job.params || "{}");
    const lessons = Array.isArray(params.lessons) ? params.lessons : [];
    const total = job.total;
    try {
      await mkdir(dir, { recursive: true, mode: 0o777 }).catch(() => {
        throw new Error("Сборка аудиодрам на сервере сейчас недоступна — попробуйте позже.");
      });
      const path = join(dir, "drama.json");
      const drama = existsSync(path)
        ? JSON.parse(await readFile(path, "utf8"))
        : {
            title: "",
            cast: [
              { id: "junior", name: params.junior, voice_hint: "молодой, быстрый, на нервах" },
              { id: "lena", name: "Лена", voice_hint: "спокойная, с иронией" },
              { id: "palych", name: "Палыч", voice_hint: "низкий, хриплый, ворчит" },
              { id: "narrator", name: "Рассказчик", voice_hint: "голос Ноа, ремарки" },
            ],
            new_terms: [],
            episodes: [],
          };
      for (let n = drama.episodes.length + 1; n <= total; n++) {
        update(job.id, { stage: "script", message: total > 1 ? `Серия ${n} из ${total}` : "" });
        const what =
          params.scope === "section"
            ? `Серия ${n} из ${total}. Урок этой серии: «${lessons[n - 1]?.title}».\nТезисы урока:\n${lessons[n - 1]?.theses ?? ""}\n\nВесь раздел по сериям:\n${lessons.map((l, i) => `${i + 1}. ${l.title}`).join("\n")}`
            : `Материал урока:\n${job.material}`;
        const before = drama.episodes.length ? `\n\nЧто было раньше:\n${previously(drama.episodes)}` : "";
        const messages = [
          { role: "system", content: rules({ ...params, total }) },
          { role: "user", content: `Тема: ${params.topic}.\n${what}${before}` },
        ];
        const parsed = await episode(job.id, messages, { n, total, minutes: params.minutes });
        if (!drama.title) drama.title = String(parsed.title || parsed.episode.title).slice(0, 120);
        for (const term of parsed.new_terms ?? []) {
          if (term?.term && !drama.new_terms.some((t) => t.term === term.term)) drama.new_terms.push(term);
        }
        drama.episodes.push({ n, ...parsed.episode });
        update(job.id, { title: drama.title, stage: "queued" });
        await write(dir, "drama.json", drama);
        await write(dir, "ready", "");
        // Ждём озвучку серии; следующую пишем, пока слушают эту.
        const deadline = Date.now() + EPISODE_LIMIT;
        for (;;) {
          await sleep(3000);
          if (Date.now() > deadline) throw new Error("Озвучка заняла слишком долго — попробуйте ещё раз.");
          try {
            const status = JSON.parse(await readFile(join(dir, "status.json"), "utf8"));
            if (STAGES[status.stage]) update(job.id, { stage: status.stage });
          } catch {
            /* служба ещё не взяла задание */
          }
          if (existsSync(join(dir, `done-${n}.json`))) break;
          if (existsSync(join(dir, "error.json"))) {
            const { log } = JSON.parse(await readFile(join(dir, "error.json"), "utf8"));
            console.error(`драма ${job.id.slice(0, 8)}: озвучка — ${String(log).slice(-300)}`);
            throw new Error("Озвучка не удалась — попробуйте ещё раз.");
          }
        }
        update(job.id, { ready: n });
      }
      update(job.id, { status: "done", stage: "done", message: "" });
    } catch (err) {
      console.error("драма:", err?.message ?? err);
      update(job.id, { status: "failed", message: String(err?.message ?? err).slice(0, 300) });
    } finally {
      running.delete(job.id);
    }
  }

  // Сервер перезапустили — недописанные дорабатываются с того места.
  for (const job of db.prepare("SELECT * FROM noa_dramas WHERE status = 'running'").all()) run(job);

  route("POST", /^\/api\/noa\/dramas$/, async ({ req, user: who, tokenUser }) => {
    const user = owner(who ?? tokenUser);
    const body = await readJson(req, 96_000);
    const course = String(body.course ?? "").slice(0, 120);
    const topic = String(body.topic ?? "").slice(0, 120);
    const scope = body.scope === "section" ? "section" : "lesson";
    const material = String(body.material ?? "").trim().slice(0, MAX_MATERIAL);
    const lessons = (Array.isArray(body.lessons) ? body.lessons : [])
      .slice(0, MAX_EPISODES)
      .map((l) => ({ title: String(l?.title ?? "").slice(0, 160), theses: String(l?.theses ?? "").slice(0, 2500) }))
      .filter((l) => l.title);
    if (!course || !topic) throw new Fail(400, "Не указан урок.");
    if (scope === "lesson" && material.length < 40) throw new Fail(400, "Не из чего делать драму: нет материала урока.");
    if (scope === "section" && lessons.length < 2) throw new Fail(400, "Для сериала нужно хотя бы два урока.");
    const params = {
      scope,
      topic: String(body.topicTitle ?? topic).slice(0, 160),
      level: body.level === "middle" ? "middle" : "junior",
      junior: String(body.junior ?? "").trim().slice(0, 30) || "Костя",
      minutes: Math.min(10, Math.max(2, Number(body.minutes) || 5)),
      lessons,
    };
    const same = db
      .prepare("SELECT * FROM noa_dramas WHERE user_id = ? AND course_id = ? AND topic_id = ? AND scope = ? ORDER BY created DESC LIMIT 1")
      .get(user.id, course, topic, scope);
    if (same && (same.status === "running" || (same.status === "done" && !body.again))) return row(same);
    const busy = db.prepare("SELECT COUNT(*) AS n FROM noa_dramas WHERE user_id = ? AND status = 'running'").get(user.id).n;
    if (busy >= 2) throw new Fail(429, "Уже пишутся две аудиодрамы — дождитесь их.");
    const id = randomUUID();
    const total = scope === "section" ? lessons.length : 1;
    db.prepare(
      "INSERT INTO noa_dramas (id, user_id, course_id, topic_id, scope, status, params, material, total) VALUES (?, ?, ?, ?, ?, 'running', ?, ?, ?)",
    ).run(id, user.id, course, topic, scope, JSON.stringify(params), material, total);
    const job = db.prepare("SELECT * FROM noa_dramas WHERE id = ?").get(id);
    run(job);
    return row(job);
  });

  route("GET", /^\/api\/noa\/dramas$/, ({ url, user: who, tokenUser }) => {
    const user = owner(who ?? tokenUser);
    const rows = db
      .prepare("SELECT * FROM noa_dramas WHERE user_id = ? AND course_id = ? AND topic_id = ? ORDER BY created DESC")
      .all(user.id, url.searchParams.get("course") ?? "", url.searchParams.get("topic") ?? "");
    const seen = new Set();
    return {
      dramas: rows
        .filter((r) => {
          if (seen.has(r.scope)) return false;
          seen.add(r.scope);
          return true;
        })
        .map(row),
    };
  });

  route("GET", /^\/api\/noa\/dramas\/([0-9a-f-]{36})$/, ({ match, user: who, tokenUser }) => {
    const user = owner(who ?? tokenUser);
    const found = db.prepare("SELECT * FROM noa_dramas WHERE id = ? AND user_id = ?").get(match[1], user.id);
    if (!found) throw new Fail(404, "Такой аудиодрамы нет.");
    return row(found);
  });

  // Сценарий с разметкой по времени — для подсветки реплик и вопросов.
  route("GET", /^\/api\/noa\/dramas\/([0-9a-f-]{36})\/script$/, async ({ match, user: who, tokenUser }) => {
    const user = owner(who ?? tokenUser);
    const found = db.prepare("SELECT * FROM noa_dramas WHERE id = ? AND user_id = ?").get(match[1], user.id);
    if (!found) throw new Fail(404, "Такой аудиодрамы нет.");
    const dir = join(JOBS, found.id);
    if (!existsSync(join(dir, "drama.json"))) throw new Fail(404, "Сценарий ещё пишется.");
    const drama = JSON.parse(await readFile(join(dir, "drama.json"), "utf8"));
    for (const ep of drama.episodes) {
      const timeline = join(dir, `timeline-${ep.n}.json`);
      ep.ready = existsSync(join(dir, `episode-${ep.n}.mp3`));
      ep.timeline = existsSync(timeline) ? JSON.parse(await readFile(timeline, "utf8")) : null;
    }
    return drama;
  });

  // Звук серии — с докачкой по Range, как ролики.
  route("GET", /^\/api\/noa\/dramas\/([0-9a-f-]{36})\/episode-(\d{1,2})\.mp3$/, ({ req, res, match, user: who, tokenUser }) => {
    const user = owner(who ?? tokenUser);
    const found = db.prepare("SELECT id FROM noa_dramas WHERE id = ? AND user_id = ?").get(match[1], user.id);
    if (!found) throw new Fail(404, "Такой аудиодрамы нет.");
    const file = join(JOBS, found.id, `episode-${Number(match[2])}.mp3`);
    if (!existsSync(file)) throw new Fail(404, "Серия ещё озвучивается.");
    const size = statSync(file).size;
    const head = {
      "Content-Type": "audio/mpeg",
      "Accept-Ranges": "bytes",
      "Cache-Control": "private, max-age=31536000, immutable",
      "X-Content-Type-Options": "nosniff",
    };
    const range = /^bytes=(\d*)-(\d*)$/.exec(String(req.headers.range ?? ""));
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
