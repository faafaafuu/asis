// Подменные команды программы для витрины приложения обучения (learn.html).
// Курсы — из dev/learn-fixtures.json (выгрузка настоящих данных тестом
// learning::dump_learn_fixtures; файл в git не идёт), без него — короткий
// пример. Ноа отвечает заготовками, терминал — эхом.
//
// Параметры: ?signed=0 — не вошли; ?build=1 — курс собирается; ?ssh=0 —
// практика просит подключиться; ?scenario=0 — практика без сценария;
// ?slow=1 — модель отвечает 6 с (видно «Медленно…»); ?offline=1 — первый
// запрос к модели падает без связи; ?empty=1 — курсов нет.

(() => {
  const params = new URLSearchParams(location.search);
  const handlers = {};
  const emit = (event, payload) => (handlers[event] ?? []).forEach((h) => h({ payload }));
  const wait = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
  const model = () => wait(params.get("slow") ? 6000 : 700);
  let offlineOnce = Boolean(params.get("offline"));
  const network = async () => {
    await model();
    if (offlineOnce) {
      offlineOnce = false;
      throw "error sending request: connection refused";
    }
  };

  const sample = {
    overview: [
      {
        id: "devops",
        title: "DevOps с нуля",
        description: "Короткий пример курса.",
        percent: 12,
        finalBest: null,
        finalUnlocked: false,
        current: "docker",
        topicPass: 80,
        finalPass: 75,
        mastery: { total: 6, new: 4, learning: 1, young: 1, mature: 0, due: 2, newLeft: 4 },
        focus: { today: 25, week: 110, streak: 5, sessionsToday: 1 },
        topics: [
          { id: "linux", title: "Linux", summary: "", status: "done", read: true, tasksDone: 1, tasksTotal: 1, examBest: 90, mistakes: 0, conceptsTotal: 3, conceptsMature: 1, step: "", section: 0 },
          { id: "docker", title: "Docker", summary: "", status: "reading", read: false, tasksDone: 0, tasksTotal: 1, examBest: null, mistakes: 0, conceptsTotal: 3, conceptsMature: 0, step: "lesson", section: 1 },
        ],
      },
    ],
    topics: {},
    exams: {},
    reviews: {},
    maps: {},
    answers: {},
  };

  let data = null;
  const fixtures = (async () => {
    try {
      const reply = await fetch("dev/learn-fixtures.json");
      if (reply.ok) data = await reply.json();
    } catch {
      /* без выгрузки — пример */
    }
    data ??= sample;
    if (params.get("empty")) data.overview = [];
    return data;
  })();

  const scenario = {
    title: "Стек в Docker Compose",
    goal: "Нода и монитор в одной сети bridge, данные в томе",
    setup: "один сервер Ubuntu 22.04 с Docker",
    picture: null,
    steps: [
      { title: "Поставить Docker из репозитория Docker", goal: "Пакеты docker-ce и compose plugin.", check: "`docker version`", concept: "образ и контейнер" },
      { title: "Пользователь без root", goal: "Работать от своего пользователя в группе docker.", check: "`docker ps` без sudo", concept: "права" },
      { title: "Каталог проекта", goal: "Создать ~/dk11 и compose.yaml.", check: "`ls ~/dk11`", concept: "compose" },
      { title: "Сеть bridge", goal: "Описать сеть node-docker_network.", check: "`docker network ls`", concept: "сети Docker" },
      { title: "Том для данных", goal: "Том для данных ноды.", check: "`docker volume ls`", concept: "тома" },
      { title: "Права контейнеров", goal: "Нода и монитор — разные пользователи.", check: "`docker compose config`", concept: "безопасность" },
      { title: "Запустить стек: docker compose up -d и проверить состояние", goal: "Подними стек и проверь, что контейнеры видят друг друга по имени.", check: "`docker compose ps && docker compose exec node ping monitor`", concept: "сети Docker, данные и сеть" },
      { title: "Проверить перезапуск", goal: "Перезапуск сервера не теряет данные.", check: "`docker compose restart`", concept: "тома" },
    ],
  };
  const practices = {};
  let practice = { course: null, topic: null, scenario: null, step: 0, done: false, feed: [], here: "", watching: true };
  let connected = !params.get("ssh");
  let line = "";
  const prompt = "\x1b[32mroot@vmi3213709:~/dk11#\x1b[0m ";
  const publish = (say = "") => emit("practice:state", { practice: structuredClone(practice), say, spoken: false });

  let speakingUntil = 0;
  let phoneVoice = "phone";
  const stats = { today: 25, week: 110, streak: 5, sessionsToday: 1 };

  async function invoke(cmd, args = {}) {
    const d = await fixtures;
    const courseOf = (id) => d.overview.find((c) => c.id === id);
    switch (cmd) {
      /* ── Курсы ── */
      case "learn_overview":
        return structuredClone(d.overview);
      case "learn_topic": {
        const view = d.topics[`${args.course}/${args.topic}`];
        if (view) return structuredClone(view);
        const card = courseOf(args.course)?.topics.find((t) => t.id === args.topic);
        return {
          topic: {
            id: args.topic,
            title: card?.title ?? args.topic,
            summary: "Тема для витрины.",
            lesson: "# Тема\n\n## Введение\n\nКонтейнер — процесс в своей коробке.\n\n```bash\ndocker run -d --name web nginx:1.27\n```\n\n> **Частая ошибка:** тег latest в проде.\n\n## Сети\n\nКонтейнеры в одной сети bridge видят друг друга по имени.",
            concepts: [{ id: "c1", term: "Контейнер", definition: "Изолированный процесс.", mnemonic: "Коробка с приложением", pitfall: "Данные внутри пропадут" }],
            exam: [],
            tasks: [],
          },
          scores: {},
          mistakes: [],
          cheatsheet: "## Конспект\n\n- `docker ps` — что запущено\n- `docker compose up -d` — поднять стек",
        };
      }
      case "learn_place":
      case "learn_focus_bell":
      case "practice_screen":
      case "practice_term_resize":
      case "practice_watch":
      case "practice_verbose":
      case "window_error":
        return null;
      case "learn_read": {
        const card = courseOf(args.course)?.topics.find((t) => t.id === args.topic);
        if (card) card.read = true;
        return null;
      }
      case "learn_map":
        return d.maps[args.course] ?? { topics: courseOf(args.course).topics.map((t) => ({ id: t.id, title: t.title, concepts: [{ id: "x", term: "Понятие", level: "learning" }] })), edges: [] };
      case "learn_review": {
        const list = args.topic ? d.reviews[`${args.course}/${args.topic}`] : d.reviews[args.course];
        if (list?.length) return structuredClone(list.slice(0, 6));
        return [
          { key: "k1", topic: "docker", topicTitle: "Docker", concept: "net", front: "Чем сеть bridge отличается от host?", back: "bridge — своя сеть контейнеров с NAT; host — контейнер в сети хоста, без изоляции портов.", mnemonic: "Мост между мирами", level: "learning", fresh: false },
          { key: "k2", topic: "docker", topicTitle: "Docker", concept: "vol", front: "Где живут данные контейнера после docker rm?", back: "Только в томе или примонтированном каталоге.", analogy: "Флешка, а не оперативка", level: "new", fresh: true },
        ];
      }
      case "learn_grade":
        return { level: "learning", next: "10 мин", again: args.grade === "again" };
      case "learn_exam": {
        const exam = d.exams[`${args.course}/${args.scope}`];
        if (exam?.questions?.length) return structuredClone(exam);
        return {
          scope: args.scope,
          title: "Проверка",
          pass: 80,
          questions: [
            { id: "q1", kind: "choice", q: "Почему контейнер web не видит контейнер db по имени?", options: ["Нужен --privileged", "Они в разных сетях", "Не открыт порт 5432", "DNS выключен в образе"] },
            { id: "q2", kind: "open", q: "Зачем закреплять версию образа вместо latest?", options: [] },
          ],
        };
      }
      case "learn_check": {
        await network();
        const known = d.answers?.[args.question];
        if (typeof args.answer === "number") {
          const answer = known?.answer ?? 1;
          const right = args.answer === answer;
          return { id: args.question, q: "", score: right ? 100 : 0, right, feedback: known?.explain || (right ? "Верно: разные сети — разные пространства имён DNS." : "Не то: дело не в правах. Контейнеры в разных сетях не видят друг друга по имени — подключите их к одной сети."), reference: known?.reference ?? "", covered: [], partial: [], points: [], options: [], chosen: args.answer, answer };
        }
        return { id: args.question, q: "", score: 70, right: true, feedback: "Главное сказали: версия не уедет сама. Не хватило про откат — с тегом его просто сделать.", reference: known?.reference || "latest меняется без вашего ведома; закреплённый тег — воспроизводимая сборка и понятный откат.", covered: [0], partial: [1], points: [], options: [], chosen: null, answer: null };
      }
      case "learn_submit": {
        await network();
        const ids = Object.keys(args.answers);
        const items = ids.map((id, i) => ({ id, q: `Вопрос ${i + 1}`, score: i % 3 ? 100 : 0, right: Boolean(i % 3), feedback: "", reference: "Эталонный ответ: что и почему.", covered: [], partial: [], points: [], options: [], chosen: null, answer: null }));
        const score = items.length ? Math.round(items.filter((v) => v.right).length * 100 / items.length) : 0;
        if (args.scope !== "final") {
          const card = courseOf(args.course)?.topics.find((t) => t.id === args.scope);
          if (card) card.examBest = Math.max(card.examBest ?? 0, score);
        }
        return { score, passed: score >= 80, pass: 80, unchecked: 0, items };
      }
      case "learn_deep":
        if (args.cached) return null;
        await network();
        return "**Подробно.** Сеть bridge — виртуальный коммутатор на хосте. Каждый контейнер получает адрес в ней, а встроенный DNS Docker отвечает на имена сервисов.\n\n```bash\ndocker network inspect node-docker_network\n```\n\n- Порты наружу — только через `ports:`\n- Внутри сети — по имени сервиса";
      case "learn_ask":
        await network();
        return "Bridge — своя сеть с NAT: контейнеры видят друг друга, наружу — только опубликованные порты. Host — контейнер прямо в сети сервера.";
      case "ai_explain":
        await network();
        return { def: `«${args.term}» — в этом абзаце это значит вот что: короткое объяснение по смыслу текста.`, simple: "", examples: [] };
      case "ai_ask":
        await network();
        return `Про «${args.term}»: уточняю — ответ на «${args.question}».`;
      case "plugin:sufler|stopListening":
        return null;
      case "phone_ask":
        await network();
        return "Сегодня по плану тема Docker: урок прочитан наполовину, две карточки ждут повторения.";
      case "learn_build":
        await network();
        return { build: { id: "b1", status: "running" } };
      case "learn_builds":
        return params.get("build") ? [{ id: "b1", status: "running", stage: "topics", done: 3, total: 9, title: "Kubernetes в проде", courseId: "k8s", message: "Пишу тему 4 из 9", goal: "", updated: "" }] : [];
      case "learn_build_stop":
        return null;
      case "learn_focus_done":
        stats.today += args.session.minutes;
        stats.week += args.session.minutes;
        stats.sessionsToday += 1;
        return { ...stats };
      case "phone_voice":
        if (args.voice) phoneVoice = args.voice;
        return phoneVoice;
      case "account_status":
        return params.get("signed") !== "0";
      case "account_login":
        await wait(1500);
        return "Владелец";
      case "app_version":
        return "1.15.0";

      /* ── Голос ── */
      case "voice_speak":
        speakingUntil = Date.now() + Math.min(6000, 600 + args.text.length * 40);
        return null;
      case "voice_stop":
        speakingUntil = 0;
        return null;
      case "voice_busy":
        return Date.now() < speakingUntil;
      case "plugin:sufler|listen":
        await wait(1200);
        return { text: "Чем сеть bridge отличается от host?" };
      case "plugin:sufler|insets":
        return { top: 0, bottom: 0, left: 0, right: 0, keyboard: 0 };
      case "plugin:sufler|cancelListening":
      case "plugin:sufler|chime":
      case "plugin:sufler|barStyle":
        return null;

      /* ── Практика ── */
      case "practice_switch": {
        const key = `${args.course}/${args.topic}`;
        practices[key] ??= {
          course: args.course,
          topic: args.topic,
          scenario: params.get("scenario") === "0" ? null : structuredClone(scenario),
          step: 6,
          done: false,
          here: "",
          watching: true,
          feed: [
            { who: "noa", kind: "step", text: "Отлично! Compose валидируется — сеть bridge создана, ноде и монитору разные права. Сейчас поднимаем стек командой `docker compose up -d`." },
            { who: "noa", kind: "hint", text: "Дальше: Ctrl+O, Enter, потом Ctrl+X — выйти из редактора." },
          ],
        };
        practice = practices[key];
        return structuredClone(practice);
      }
      case "practice_state":
        return structuredClone(practice);
      case "practice_progress":
        return Object.values(practices)
          .filter((p) => p.scenario)
          .map((p) => ({ course: p.course, topic: p.topic, step: p.step, total: p.scenario.steps.length, done: p.done }));
      case "practice_server":
        return { host: "203.0.113.10", port: 22, user: "root", known: "", publicKey: "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIJexampleexampleexampleexampleexample noa-practice" };
      case "practice_term_start":
        if (!connected) throw "ssh:connect";
        setTimeout(() => emit("practice:out", "  stop_grace_period: 30s\r\nnetworks:\r\n  node-docker_network: driver: bridge\r\n" + prompt), 30);
        return "";
      case "practice_connect":
        await wait(700);
        if (!args.host) throw "Впишите адрес сервера и пользователя.";
        connected = true;
        setTimeout(() => emit("practice:out", `Welcome to Ubuntu 22.04 LTS (${args.host})\r\n\r\n` + prompt), 30);
        return null;
      case "practice_term_write":
        if (args.data === "\r") {
          const out = line.trim() === "docker compose ps" ? "\r\nNAME      IMAGE          STATUS\r\nnode      node:20.16     Up 2 min\r\nmonitor   grafana:11.1   Up 2 min\r\n" : "\r\n";
          line = "";
          emit("practice:out", out + prompt);
        } else if (args.data === "\x7f") {
          if (line) {
            line = line.slice(0, -1);
            emit("practice:out", "\b \b");
          }
        } else {
          line += args.data;
          emit("practice:out", args.data);
        }
        return null;
      case "practice_plan":
        await wait(1500);
        practice.scenario = structuredClone(scenario);
        practice.step = 0;
        practice.feed.push({ who: "noa", kind: "step", text: `Шаг 1 из ${scenario.steps.length}: ${scenario.steps[0].title}.` });
        return structuredClone(practice);
      case "practice_ask":
        practice.feed.push({ who: "me", kind: "answer", text: args.text });
        publish();
        await network();
        practice.feed.push({ who: "noa", kind: "answer", text: "Сейчас стек описан в compose.yaml. Поднимите его `docker compose up -d` и проверьте `docker compose ps`." });
        publish("Сейчас стек описан. Поднимите его и проверьте состояние.");
        return "";
      case "practice_check":
        await model();
        if (practice.step + 1 >= practice.scenario.steps.length) practice.done = true;
        else practice.step += 1;
        practice.feed.push({ who: "noa", kind: "step", text: "Вижу: оба контейнера Up и монитор отвечает на ping по имени. Шаг сделан." });
        publish("Шаг сделан.");
        return null;
      case "practice_step":
        practice.step = Math.max(0, Math.min(practice.scenario.steps.length - 1, practice.step + args.delta));
        publish();
        return null;
      case "practice_reset":
        practice.scenario = null;
        practice.step = 0;
        practice.done = false;
        publish();
        return null;
      default:
        console.warn("витрина: нет подмены для", cmd, args);
        return null;
    }
  }

  globalThis.__TAURI__ = {
    core: { invoke },
    event: {
      listen: async (event, handler) => {
        (handlers[event] ??= []).push(handler);
        return () => {};
      },
    },
  };
})();
