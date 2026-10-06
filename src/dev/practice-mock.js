// Подменные команды программы для витрины окна практики: терминал отвечает
// эхом и выдуманным выводом, Ноа — заготовленными замечаниями.

(() => {
  const params = new URLSearchParams(location.search);
  if (params.get("theme")) document.documentElement.dataset.theme = params.get("theme");
  const handlers = {};
  const emit = (event, payload) => (handlers[event] ?? []).forEach((h) => h({ payload }));

  const scenario = {
    title: "Кластер k3s из трёх узлов",
    goal: "Три сервера в одном кластере, в нём работает nginx и открывается снаружи",
    setup: "три сервера Ubuntu 22.04, доступ root по SSH",
    picture: {
      title: "Что строим",
      points: ["Сервер управляет, агенты выполняют", "Под живёт на узле", "Сервис открывает под наружу"],
      nodes: [
        { id: "you", label: "Ваш компьютер" },
        { id: "server", label: "k3s server" },
        { id: "agent", label: "Агенты (2)" },
        { id: "pod", label: "Под nginx" },
        { id: "svc", label: "Service NodePort" },
      ],
      edges: [
        { from: "you", to: "server", label: "kubectl" },
        { from: "server", to: "agent", label: "задачи" },
        { from: "agent", to: "pod", label: "запуск" },
        { from: "pod", to: "svc", label: "" },
      ],
    },
    steps: [
      { title: "Подготовить серверы", goal: "Обновить пакеты и задать имена узлам.", why: "Узлы узнают друг друга по именам.", check: "`hostnamectl` показывает новое имя", node: "server", concept: "узел" },
      { title: "Поставить k3s server", concept: "control plane", goal: "На первом сервере запустить управляющий узел.", why: "Он хранит состояние кластера и раздаёт задачи.", check: "`kubectl get nodes` — один узел Ready", node: "server" },
      { title: "Подключить агентов", goal: "Два других сервера присоединить по токену.", why: "На агентах запускаются поды.", check: "три узла Ready", node: "agent" },
      { title: "Запустить nginx", goal: "Создать Deployment с nginx.", why: "Первое приложение в кластере.", check: "под Running", node: "pod" },
      { title: "Открыть наружу", goal: "Сервис NodePort и проверка из браузера.", why: "Без сервиса под виден только внутри.", check: "страница nginx по адресу узла", node: "svc" },
    ],
  };
  let state = {
    course: "k8s",
    scenario: params.get("empty") ? null : scenario,
    step: 1,
    done: false,
    here: "server",
    watching: true,
    topic: "k3s",
    feed: params.get("empty")
      ? []
      : [
          { who: "noa", kind: "step", text: "Шаг 2 из 5: Поставить k3s server. На первом сервере запустить управляющий узел." },
          { who: "noa", kind: "comment", text: "Вы зашли на первый сервер по SSH — дальше всё, что набираете, выполняется уже на нём." },
          { who: "noa", kind: "error", text: "curl не найден: на чистой Ubuntu его нет. Поставьте пакетом и повторите." },
          { who: "noa", kind: "hint", text: "Дальше: `apt install -y curl`, потом установщик k3s." },
          { who: "me", kind: "answer", text: "а зачем вообще отдельный server и agent?" },
          { who: "noa", kind: "answer", text: "Server — мозг: хранит, что где должно работать, и раздаёт задачи. Agent — руки: запускает поды. Разделяют, чтобы мозг не падал от нагрузки приложений." },
        ],
  };

  const items = { "k8s/k3s": state };
  let line = "";
  const prompt = "\x1b[32mroot@node1\x1b[0m:~# ";
  globalThis.__TAURI__ = {
    core: {
      invoke: async (cmd, args = {}) => {
        switch (cmd) {
          case "practice_term_start":
            setTimeout(() => emit("practice:out", "Welcome to Ubuntu 22.04 LTS\r\n\r\n" + prompt), 50);
            return "";
          case "practice_term_write":
            if (args.data === "\r") {
              const out = line.trim() === "kubectl get nodes" ? "\r\nNAME    STATUS   ROLES                  AGE   VERSION\r\nnode1   Ready    control-plane,master   2m    v1.30.4+k3s1\r\n" : "\r\n";
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
          case "practice_state":
            return structuredClone(state);
          case "practice_plan":
            await new Promise((r) => setTimeout(r, 900));
            state.scenario = scenario;
            state.step = 0;
            return structuredClone(state);
          case "practice_ask":
            state.feed.push({ who: "me", kind: "answer", text: args.text });
            await new Promise((r) => setTimeout(r, 700));
            state.feed.push({ who: "noa", kind: args.text.startsWith("Общая") ? "overview" : "answer", text: "Сейчас вы на управляющем узле: k3s server поставлен, дальше подключим агентов по токену." });
            emit("practice:state", { practice: structuredClone(state), say: "", spoken: false });
            return "";
          case "practice_step":
            state.step = Math.max(0, state.step + args.delta);
            state.here = scenario.steps[state.step]?.node ?? "";
            emit("practice:state", { practice: structuredClone(state), say: "", spoken: false });
            return null;
          case "practice_watch":
            state.watching = args.on;
            emit("practice:state", { practice: structuredClone(state), say: "", spoken: false });
            return null;
          case "practice_verbose":
            state.verbose = args.on;
            emit("practice:state", { practice: structuredClone(state), say: "", spoken: false });
            return null;
          case "practice_check":
            await new Promise((r) => setTimeout(r, 700));
            state.feed.push({ who: "noa", kind: "comment", text: "Пока не вижу в терминале, что шаг сделан: kubectl get nodes ещё не запускали." });
            emit("practice:state", { practice: structuredClone(state), say: "", spoken: false });
            return null;
          case "practice_switch": {
            const key = `${args.course}/${args.topic}`;
            items[key] ??= { course: args.course, topic: args.topic, scenario: null, step: 0, done: false, here: "", watching: state.watching, verbose: state.verbose, feed: [] };
            state = items[key];
            return structuredClone(state);
          }
          case "practice_progress":
            return Object.values(items)
              .filter((p) => p.scenario)
              .map((p) => ({ course: p.course, topic: p.topic, step: p.step, total: p.scenario.steps.length, done: p.done }));
          case "learn_overview":
            return [
              {
                id: "k8s",
                title: "Kubernetes с нуля",
                current: "k3s",
                topics: [
                  { id: "containers", title: "Контейнеры и образы", status: "done" },
                  { id: "k3s", title: "Кластер k3s: server и agent", status: "reading" },
                  { id: "deploy", title: "Deployment и Service", status: "new" },
                  { id: "ingress", title: "Ingress и HTTPS", status: "new" },
                ],
              },
              { id: "net", title: "Сети для админа", current: null, topics: [{ id: "osi", title: "Модель OSI", status: "new" }] },
            ];
          case "runtime_config":
            return { theme: document.documentElement.dataset.theme };
          case "app_version":
            return "1.15.0";
          default:
            return false;
        }
      },
    },
    event: {
      listen: async (event, handler) => {
        (handlers[event] ??= []).push(handler);
        return () => {};
      },
    },
  };
})();
