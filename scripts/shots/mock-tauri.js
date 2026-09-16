// Подмена Tauri для кадров README: окна программы получают тестовые данные
// вместо настоящих. Ничего личного на картинки не попадает.
//
// Обычный (не модульный) скрипт: он должен выполниться до того, как окно
// спросит `globalThis.__TAURI__`.

(() => {
  const DAY = 86_400_000;
  const at = (days, hours, minutes = 0) => {
    const d = new Date(Date.now() + days * DAY);
    d.setHours(hours, minutes, 0, 0);
    return d.toISOString();
  };

  /** Плавная кривая с шумом — для маленьких графиков. */
  const spark = (seed, trend) =>
    Array.from({ length: 48 }, (_, i) => 100 + trend * i + 6 * Math.sin(i / 4 + seed) + 3 * Math.sin(i * 1.7 + seed * 3));

  const asset = (id, kind, symbol, name, price, currency, changes, tabs, seed, alerts = []) => ({
    id,
    kind,
    symbol,
    name,
    price,
    currency,
    day: changes[0],
    week: changes[1],
    month: changes[2],
    year: changes[3],
    all: changes[4],
    spark: spark(seed, changes[1] / 10),
    tabs,
    alerts,
  });

  const ROWS = [
    asset("bitcoin", "crypto", "BTC", "Bitcoin", 97350, "USD", [1.8, 4.2, -3.1, 58.4, 1250], ["Избранное", "Крипто"], 1, [
      { price: 100000, above: true, fired: null },
    ]),
    asset("ethereum", "crypto", "ETH", "Ethereum", 3412, "USD", [-0.9, 2.7, 6.3, 21.5, 812], ["Крипто"], 2),
    asset("solana", "crypto", "SOL", "Solana", 186.4, "USD", [3.4, 9.1, 14.2, 96.8, 4210], ["Крипто"], 3, [
      { price: 150, above: false, fired: null },
    ]),
    asset("AAPL", "stock", "AAPL", "Apple", 231.6, "USD", [0.4, -1.2, 2.9, 18.7, 640], ["Избранное"], 4),
    asset("SBER", "moex", "SBER", "Сбербанк", 312.45, "RUB", [-0.6, 1.1, 4.8, 12.3, 355], ["Избранное", "РФ фонды"], 5),
    asset("EURUSD=X", "stock", "EURUSD", "Евро / доллар", 1.1596, "USD", [0.1, -0.3, 0.8, 4.2, -12.4], [], 6),
  ];

  const TASKS = [
    { id: "t1", title: "Оплатить интернет", due: at(-1, 18), done: false, overdue: true, steps: [], advice: null, postponed: 0, inCalendar: false },
    { id: "t2", title: "Позвонить в банк", due: at(0, 23, 30), done: false, overdue: false, steps: [], advice: null, postponed: 0, inCalendar: true },
    {
      id: "t3",
      title: "Обновить резюме",
      due: at(1, 10),
      done: false,
      overdue: false,
      steps: [
        { title: "Собрать проекты за последний год", done: true },
        { title: "Переписать раздел «Опыт» под DevOps", done: false },
        { title: "Отправить на ревью другу", done: false },
      ],
      advice: "Начните с проектов: из них раздел «Опыт» пишется почти сам.",
      postponed: 0,
      inCalendar: false,
    },
    { id: "t4", title: "Записаться к стоматологу", due: at(3, 18), done: false, overdue: false, steps: [], advice: null, postponed: 3, inCalendar: false },
    { id: "t5", title: "Разобрать Kubernetes Operators", due: null, done: false, overdue: false, steps: [], advice: null, postponed: 0, inCalendar: false },
    { id: "t6", title: "Уборка", due: at(0, 9), done: true, overdue: false, steps: [], advice: null, postponed: 0, inCalendar: false },
  ];

  const COURSE_TOPICS = ["linux", "network", "git", "bash", "docker", "kubernetes", "cicd", "iac", "cloud", "monitoring", "security", "sre"];
  const STATUS = { linux: "done", network: "done", git: "done", bash: "practice", docker: "reading" };
  const base = new URL("../../src-tauri/courses/devops/", document.baseURI);
  const load = (name) => fetch(new URL(`${name}.json`, base)).then((r) => r.json());

  async function overview() {
    const course = await load("course");
    const topics = await Promise.all(COURSE_TOPICS.map(load));
    return [
      {
        id: course.id,
        title: course.title,
        description: course.description,
        topics: topics.map((t) => {
          const status = STATUS[t.id] ?? "new";
          const total = t.tasks.length;
          return {
            id: t.id,
            title: t.title,
            summary: t.summary,
            status,
            read: status !== "new",
            tasksDone: status === "done" ? total : status === "practice" ? 1 : 0,
            tasksTotal: total,
            examBest: status === "done" ? 80 + t.id.length : null,
            mistakes: status === "practice" ? 2 : 0,
          };
        }),
        percent: 31,
        finalBest: null,
        finalUnlocked: false,
        current: "docker",
        topicPass: 70,
        finalPass: 75,
      },
    ];
  }

  async function topic(id) {
    const t = await load(id);
    return { topic: { ...t, exam: [] }, scores: {}, mistakes: [] };
  }

  async function check(questionId) {
    for (const name of COURSE_TOPICS) {
      const t = await load(name);
      const q = t.tasks.find((task) => task.id === questionId);
      if (!q) continue;
      return {
        id: q.id,
        q: q.q,
        score: 45,
        right: false,
        feedback: "Главное названо, но не сказано, что слои кешируются и порядок инструкций на это влияет.",
        reference: q.reference || q.explain || "",
        covered: q.points?.length ? [1] : [],
        points: q.points ?? [],
        options: q.options ?? [],
        chosen: null,
        answer: q.answer ?? null,
      };
    }
    return null;
  }

  const HANDLERS = {
    runtime_config: () => ({
      theme: new URLSearchParams(location.search).get("theme") ?? "neon",
      errorText: "",
      dialogue: true,
      mobile: false,
      language: "ru",
    }),
    watch_rows: () => ROWS,
    watch_tabs: () => ["Избранное", "Крипто", "РФ фонды"],
    watch_telegram_ready: () => true,
    watch_open_tab: () => null,
    task_list: () => TASKS,
    learn_overview: overview,
    learn_topic: (args) => topic(args.topic),
    learn_check: (args) => check(args.question),
  };

  const MODULES = [
    { id: "tasks", title: "Задачи", icon: "✓", about: "Дела со сроками, шаги и напоминания", status: "5 дел, просрочено 1", voice: "«Ноа, напомни завтра в десять позвонить в банк»", window: true },
    { id: "watchlist", title: "Активы", icon: "◆", about: "Крипта, акции, валюты и оповещения о цене", status: "6 активов · 2 оповещения", voice: "«Ноа, поставь алерт на биткоин на 100 тысяч»", window: true },
    { id: "learning", title: "Обучение", icon: "◈", about: "Курсы с уроками, задачами и экзаменами", status: "DevOps — 31%", voice: "«Ноа, погоняй меня по докеру»", window: true },
    { id: "order", title: "Заказы", icon: "▣", about: "Продукты по лучшей цене, корзина одним голосом", status: "3 товара в корзине, ВкусВилл", voice: "«Ноа, закажи молоко, хлеб и яйца»", window: true },
    { id: "alarms", title: "Будильники", icon: "◷", about: "Будильники по дням недели и таймеры", status: "2 будильника, ближний на 07:00", voice: "«Ноа, разбуди в семь по будням»", window: false },
    { id: "telegram", title: "Telegram", icon: "➤", about: "Ноа в мессенджере: текстом и голосовыми", status: "подключён", voice: "Пишите своему боту — отвечает тем же", window: false },
  ];
  Object.assign(HANDLERS, {
    modules_overview: () => MODULES,
    ai_settings: () => ({ provider: "http", endpoint: "http://127.0.0.1:11434/api/chat", apiKey: "", model: "qwen3.5:4b", proxy: "" }),
    voice_settings: () => ({ enabled: true, engine: "silero", voice: "ru_RU-irina-medium", edgeVoice: "ru-RU-SvetlanaNeural", sileroVoice: "xenia", wakeWord: true, inputDevice: "", rate: 1.2, speakAnswers: false, ready: true, azureKey: "", azureRegion: "westeurope" }),
    voice_list: () => ({
      piper: [{ id: "ru_RU-irina-medium", label: "Ирина — женский, спокойный" }],
      azure: [],
      silero: [{ id: "xenia", label: "Ксения — женский, живой" }, { id: "aidar", label: "Айдар — мужской" }],
    }),
    integration_status: () => ({ kind: "ready" }),
    local_models: () => ({
      running: true,
      present: true,
      installed: [
        { name: "qwen3.5:4b", sizeGb: 3.4 },
        { name: "qwen3.5:9b", sizeGb: 6.6 },
      ],
    }),
    recommended_model: () => "qwen3.5:4b",
    usage_summary: () => ({
      today: { requests: 46, prompt: 118400, completion: 6150, cost: 0.0214 },
      month: { requests: 1210, prompt: 3120000, completion: 162000, cost: 0.587 },
      total: { requests: 2890, prompt: 7450000, completion: 391000, cost: 1.42 },
      balance: 9.41,
      spent: 1.59,
      model: "deepseek/deepseek-v4-flash",
      cloud: true,
      service: "OpenRouter",
    }),
    widget_settings: () => ({ enabled: true, onTop: false }),
    plugins_library: async () =>
      (await fetch(new URL("../../modules/index.json", document.baseURI)).then((r) => r.json())).modules,
  });
  // Тема окна — так же, как её подставляет программа до первого кадра.
  globalThis.__SUFLER_VIEW__ = {
    theme: new URLSearchParams(location.search).get("theme") ?? "neon",
    language: "ru",
  };

  const noop = () => Promise.resolve();
  globalThis.__TAURI__ = {
    core: {
      invoke: async (cmd, args = {}) => (HANDLERS[cmd] ? HANDLERS[cmd](args) : null),
    },
    event: { listen: async () => () => {} },
    window: {
      getCurrentWindow: () => ({
        minimize: noop,
        close: noop,
        hide: noop,
        startDragging: noop,
        startResizeDragging: noop,
        setSize: noop,
        onResized: async () => () => {},
      }),
    },
  };
})();
