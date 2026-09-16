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
    asset("AAPL", "stock", "AAPL", "Apple", 231.6, "USD", [0.4, -1.2, 2.9, 18.7, 640], ["Избранное"], 4, [
      { price: 250, above: true, fired: null },
    ]),
    asset("MSFT", "stock", "MSFT", "Microsoft", 468.2, "USD", [0.9, 2.1, 3.4, 22.1, 910], ["Избранное"], 1),
    asset("EURUSD=X", "stock", "EURUSD", "Евро / доллар", 1.1596, "USD", [0.1, -0.3, 0.8, 4.2, -12.4], ["Валюты"], 6),
    asset("GC=F", "stock", "GOLD", "Золото", 2410, "USD", [0.3, 1.4, 2.2, 19.8, 230], ["Избранное"], 2),
  ];

  const TASKS = [
    { id: "t1", title: "Оплатить интернет", due: at(-1, 18), done: false, overdue: true, steps: [], advice: null, postponed: 0, inCalendar: false },
    { id: "t2", title: "Забрать посылку", due: at(0, 19, 30), done: false, overdue: false, steps: [], advice: null, postponed: 0, inCalendar: true },
    {
      id: "t3",
      title: "Подготовить доклад",
      due: at(1, 10),
      done: false,
      overdue: false,
      steps: [
        { title: "Собрать материалы", done: true },
        { title: "Набросать план на пять слайдов", done: false },
        { title: "Прогнать вслух за десять минут", done: false },
      ],
      advice: "Начните с плана: по нему слайды собираются почти сами.",
      postponed: 0,
      inCalendar: false,
    },
    { id: "t4", title: "Полить цветы", due: at(3, 18), done: false, overdue: false, steps: [], advice: null, postponed: 2, inCalendar: false },
    { id: "t5", title: "Выбрать подарок", due: null, done: false, overdue: false, steps: [], advice: null, postponed: 0, inCalendar: false },
    { id: "t6", title: "Уборка", due: at(0, 9), done: true, overdue: false, steps: [], advice: null, postponed: 0, inCalendar: false },
  ];

  // Курс для кадров — выдуманный, к курсам программы отношения не имеет.
  const question = (id, q, points, reference) => ({ id, kind: "open", q, options: [], answer: null, explain: "", points, reference });
  const LESSON = [
    "# Термодинамика",
    "",
    "Термодинамика описывает, как теплота превращается в работу и обратно.",
    "",
    "## Первое начало",
    "",
    "Теплота, переданная системе, идёт на изменение её внутренней энергии и на работу против внешних сил: **Q = ΔU + A**. Это закон сохранения энергии для тепловых процессов.",
    "",
    "## Второе начало",
    "",
    "Теплота сама переходит только от горячего к холодному. Энтропия замкнутой системы не убывает, поэтому тепловой двигатель всегда отдаёт часть теплоты холодильнику.",
    "",
    "## Цикл Карно",
    "",
    "Идеальный цикл из двух изотерм и двух адиабат. Его КПД зависит только от температур нагревателя и холодильника: **η = 1 − T₂ / T₁**.",
  ].join("\n");
  const PHYSICS = [
    { id: "mechanics", title: "Механика", summary: "Движение, силы и законы Ньютона", status: "done" },
    { id: "molecular", title: "Молекулярная физика", summary: "Газы, давление и температура", status: "done" },
    {
      id: "thermo",
      title: "Термодинамика",
      summary: "Теплота, работа и энтропия",
      status: "practice",
      lesson: LESSON,
      tasks: [
        question(
          "th-1",
          "Почему невозможен вечный двигатель второго рода?",
          ["Энтропия замкнутой системы не убывает", "Часть теплоты неизбежно уходит холодильнику"],
          "Второе начало запрещает процесс, единственный итог которого — превращение теплоты целиком в работу: часть теплоты всегда уходит холодильнику, а энтропия замкнутой системы не убывает.",
        ),
        question("th-2", "Чему равен КПД цикла Карно при 500 K и 300 K?", ["η = 1 − T₂/T₁", "Ответ — 40%"], "η = 1 − 300/500 = 0,4, то есть 40%."),
        question("th-3", "Что происходит с внутренней энергией газа при адиабатном сжатии?", ["Теплообмена нет", "Работа над газом увеличивает энергию"], "Теплообмена нет, работа внешних сил целиком идёт на рост внутренней энергии — газ нагревается."),
      ],
    },
    { id: "electricity", title: "Электричество", summary: "Заряд, поле, ток и цепи", status: "reading" },
    { id: "magnetism", title: "Магнетизм", summary: "Магнитное поле и индукция", status: "new" },
    { id: "optics", title: "Оптика", summary: "Свет, линзы и интерференция", status: "new" },
    { id: "atomic", title: "Атомная физика", summary: "Строение атома и спектры", status: "new" },
  ];
  const lessonOf = (t) => t.lesson ?? `# ${t.title}\n\n${t.summary}.`;
  const tasksOf = (t) => t.tasks ?? [question(`${t.id}-1`, `Главная идея темы «${t.title}»?`, ["Суть темы"], t.summary)];

  async function overview() {
    return [
      {
        id: "physics",
        title: "Общая физика",
        description: "Ознакомительный курс: от механики до атома.",
        topics: PHYSICS.map((t) => {
          const total = tasksOf(t).length;
          return {
            id: t.id,
            title: t.title,
            summary: t.summary,
            status: t.status,
            read: t.status !== "new",
            tasksDone: t.status === "done" ? total : t.status === "practice" ? 1 : 0,
            tasksTotal: total,
            examBest: t.status === "done" ? 86 : null,
            mistakes: t.status === "practice" ? 1 : 0,
          };
        }),
        percent: 31,
        finalBest: null,
        finalUnlocked: false,
        current: "thermo",
        topicPass: 70,
        finalPass: 75,
      },
    ];
  }

  async function topic(id) {
    const t = PHYSICS.find((x) => x.id === id) ?? PHYSICS[2];
    return {
      topic: { id: t.id, title: t.title, aliases: [], summary: t.summary, lesson: lessonOf(t), tasks: tasksOf(t), exam: [] },
      scores: {},
      mistakes: [],
    };
  }

  async function check(questionId) {
    const q = PHYSICS.flatMap(tasksOf).find((task) => task.id === questionId);
    if (!q) return null;
    return {
      id: q.id,
      q: q.q,
      score: 55,
      right: false,
      feedback: "Про холодильник сказано верно, но не хватает главного — роста энтропии замкнутой системы.",
      reference: q.reference,
      covered: [1],
      points: q.points,
      options: [],
      chosen: null,
      answer: null,
    };
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
    watch_tabs: () => ["Избранное", "Валюты"],
    watch_telegram_ready: () => true,
    watch_open_tab: () => null,
    task_list: () => TASKS,
    learn_overview: overview,
    learn_topic: (args) => topic(args.topic),
    learn_check: (args) => check(args.question),
  };

  const MODULES = [
    { id: "tasks", title: "Задачи", icon: "✓", about: "Дела со сроками, шаги и напоминания", status: "5 дел, просрочено 1", voice: "«Ноа, напомни завтра в десять забрать посылку»", window: true },
    { id: "learning", title: "Обучение", icon: "◈", about: "Курсы с уроками, задачами и экзаменами", status: "Общая физика — 31%", voice: "«Ноа, погоняй меня по термодинамике»", window: true },
    { id: "order", title: "Заказы", icon: "▣", about: "Продукты по лучшей цене, корзина одним голосом", status: "3 товара в корзине", voice: "«Ноа, закажи молоко, хлеб и яйца»", window: true },
    { id: "alarms", title: "Будильники", icon: "◷", about: "Будильники по дням недели и таймеры", status: "2 будильника, ближний на 07:00", voice: "«Ноа, разбуди в семь по будням»", window: false },
    { id: "watchlist", title: "Активы", icon: "◆", about: "Акции, валюты и оповещения о цене", status: "4 позиции · 1 оповещение", voice: "«Ноа, какой курс евро»", window: true },
    { id: "telegram", title: "Telegram", icon: "➤", about: "Ноа в мессенджере: текстом и голосовыми", status: "подключён", voice: "Пишите своему боту — отвечает тем же", window: false },
  ];
  Object.assign(HANDLERS, {
    modules_overview: () => MODULES,
    ai_settings: () => ({ provider: "http", endpoint: "http://127.0.0.1:11434/api/chat", apiKey: "", model: "qwen3.5:4b", proxy: "" }),
    voice_settings: () => ({ enabled: true, engine: "silero", voice: "ru_RU-irina-medium", edgeVoice: "ru-RU-SvetlanaNeural", sileroVoice: "xenia", wakeWord: true, wakeName: "", inputDevice: "", rate: 1.2, speakAnswers: false, ready: true, azureKey: "", azureRegion: "westeurope" }),
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
    default_wake_name: () => "Ноа",
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
