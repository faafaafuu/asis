// Команды урока — набор в конце, чтобы попробовать руками: команда и что она
// делает. Берутся из самого урока: строки блоков кода и `команды` в тексте;
// описание — комментарий к строке или фраза урока, где команда упомянута.
// Один набор — на компьютере, телефоне и в Ноа онлайн.

/** Похоже на команду терминала: слово из латиницы и аргументы. */
const COMMAND = /^(sudo\s+)?[a-z][a-z0-9._-]*(\s+\S+)+$/;
/** Утилиты, с которых начинается команда в тексте: «`logs --previous`» без них — обрывок, а не команда. */
const TOOLS = new Set(
  ("docker docker-compose podman kubectl helm minikube kind k9s git systemctl journalctl sudo ls cat grep ps top htop df du free ip ss netstat " +
    "curl wget ping dig nslookup traceroute mtr chmod chown ssh scp rsync apt apt-get yum dnf pip pip3 npm npx node python python3 terraform " +
    "ansible ansible-playbook kill pkill tail head less nano vim tar find crontab ufw iptables nft nginx openssl mkdir rm cp mv echo export env " +
    "uname lsof strace vmstat iostat mount umount lsblk fdisk mkfs useradd usermod passwd su hostnamectl timedatectl nc telnet tcpdump " +
    "geth solana bitcoin-cli promtool amtool redis-cli psql mysql mongosh sqlite3 make go cargo uptime who whoami id watch").split(" "),
);

/** Не команды: вывод, YAML, пути без действия, присваивания. */
const NOT_COMMAND = /^(\w+:\s|[-#>|{}[\]]|\$\{|[A-Z_]+=|https?:)/;

/** Предложения абзаца. */
function sentences(text) {
  return text
    .replace(/\s+/g, " ")
    .split(/(?<=[.!?])\s+(?=[А-ЯЁA-Z«])/)
    .map((s) => s.trim())
    .filter(Boolean);
}

/**
 * Описание из фразы урока. Во фразе часто перечислено несколько команд —
 * «`docker stats` — потребление, `docker system prune` — уборка», — поэтому
 * берётся кусок про эту команду: пояснение после тире или в скобках. Нет
 * пояснения — заголовок перечня («Диагностика: …»), иначе ничего.
 */
function describe(sentence, command) {
  const plain = (text) => text.replace(/`([^`]*)`/g, "$1").replace(/\*\*|__/g, "").replace(/^[-*•]\s+/, "").trim();
  const marked = `\`${command}\``;
  const at = sentence.indexOf(marked);
  if (at < 0) return "";
  const after = sentence.slice(at + marked.length);
  const dash = /^\s*[—–-]\s*([^,;`]+?)(?=[,;]|$|\.\s|\.$)/.exec(after);
  if (dash) return capital(plain(dash[1]));
  const paren = /^\s*\(([^)`]+)\)/.exec(after);
  if (paren) return capital(plain(paren[1]));
  const others = (sentence.match(/`[^`]+`/g) ?? []).length;
  if (others === 1) {
    const whole = plain(sentence.replace(marked, "эта команда"));
    return whole.length > 220 ? `${whole.slice(0, 217).trimEnd()}…` : capital(whole);
  }
  const head = /^([^:`]{3,60}):/.exec(sentence);
  return head ? capital(plain(head[1])) : "";
}

const capital = (text) => (text ? text[0].toUpperCase() + text.slice(1) : text);

/**
 * Команды урока: `[{ command, about }]`, без повторов, до `limit`.
 */
export function lessonCommands(lesson, limit = 14) {
  const out = [];
  const seen = new Set();
  const add = (command, about, inline = false) => {
    const clean = command.replace(/^\$\s*/, "").trim();
    if (!clean || clean.length > 140 || seen.has(clean) || NOT_COMMAND.test(clean) || !COMMAND.test(clean)) return;
    if (inline && !TOOLS.has(clean.replace(/^sudo\s+/, "").split(/\s+/)[0])) return;
    seen.add(clean);
    out.push({ command: clean, about: about || "" });
  };
  const text = String(lesson ?? "");
  // Блоки кода: строка — команда, комментарий — её описание; без
  // комментария — фраза перед блоком.
  const blocks = /```[^\n]*\n([\s\S]*?)```/g;
  let last = 0;
  for (let match; (match = blocks.exec(text)); ) {
    const before = sentences(text.slice(last, match.index).split(/\n\s*\n/).pop() ?? "").pop() ?? "";
    for (const line of match[1].split("\n")) {
      const [code, comment] = line.split(/\s+#\s+/);
      add(code ?? "", comment ? comment.trim() : describe(before, (code ?? "").trim()));
    }
    last = match.index + match[0].length;
  }
  // `команда` в тексте — с фразой, где она упомянута.
  const prose = text.replace(blocks, "\n");
  for (const paragraph of prose.split(/\n\s*\n|\n(?=[-*•]\s)/)) {
    for (const sentence of sentences(paragraph)) {
      for (const inline of sentence.matchAll(/`([^`\n]+)`/g)) add(inline[1], describe(sentence, inline[1]), true);
    }
  }
  return out.slice(0, limit);
}

/**
 * Блок «Команды урока» для окна: команда моноширинно, описание под ней,
 * «⧉» — скопировать. Сразу — команды из текста урока; набор от модели
 * (`learn_commands`, с описаниями) заменяет их, когда придёт.
 * `el(tag, class, text)` — конструктор окна.
 */
export function commandsBox({ api, el, course, topic, lesson }) {
  const box = el("details", "commands");
  box.open = true;
  const title = el("summary", "commands__title");
  const items = el("ul", "commands__list");
  box.append(title, items);
  const draw = (list) => {
    title.textContent = `Команды урока · ${list.length} — попробуйте руками`;
    items.replaceChildren(...list.map((entry) => commandItem(entry, el)));
    box.hidden = !list.length;
  };
  draw(lessonCommands(lesson));
  api
    ?.invoke("learn_commands", { course, topic })
    .then((list) => Array.isArray(list) && list.length && box.isConnected !== false && draw(list))
    .catch(() => {
      /* без модели — остаются команды из текста урока */
    });
  return box;
}

function commandItem({ command, about, risky }, el) {
  const item = el("li", `commands__item${risky ? " is-risky" : ""}`);
  const line = el("div", "commands__line");
  const code = el("code", "commands__code", command);
  const copy = el("button", "commands__copy", "⧉");
  copy.type = "button";
  copy.title = "Скопировать";
  copy.addEventListener("click", async () => {
    try {
      await navigator.clipboard.writeText(command);
      copy.textContent = "✓";
      setTimeout(() => (copy.textContent = "⧉"), 1200);
    } catch {
      /* буфер недоступен — команду видно и так */
    }
  });
  line.append(code, copy);
  item.append(line);
  if (about) item.append(el("span", "commands__about", `${risky ? "⚠ " : ""}${about}`));
  return item;
}
