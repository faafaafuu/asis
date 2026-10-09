// Плашка клавиш терминала на телефоне и защита от опасных команд.
//
// На клавиатуре iPhone нет Esc, Tab, Ctrl и стрелок — без них не выйти из
// nano, не прервать команду, не листать историю. Одна плашка над
// клавиатурой: самое частое — стрелки, «Вставить», Esc, Tab, Ctrl, Ctrl+C;
// открыт nano или vim — первыми их кнопки; у шага есть проверка — её
// команда (вписывается, выполняет ваш Enter).

import { el, sheet, api } from "./core.js";

const ESC = "\x1b";

/** Плашка: подпись, последовательность (null — Ctrl, залипает; "paste" — вставка). */
const KEYS = [
  ["↑", `${ESC}[A`, "Предыдущая команда"],
  ["↓", `${ESC}[B`, "Следующая команда"],
  ["Вставить", "paste"],
  ["Tab", "\t", "Дописать имя"],
  ["Esc", ESC],
  ["Ctrl", null],
  ["^C", "\x03", "Прервать команду"],
  ["←", `${ESC}[D`],
  ["→", `${ESC}[C`],
  ["|", "|"],
  ["~", "~"],
  ["/", "/"],
  ["-", "-"],
  ["^D", "\x04", "Конец ввода / выйти"],
  ["^L", "\x0c", "Очистить экран"],
  ["^R", "\x12", "Поиск по истории"],
];

/** Кнопки редактора, открытого на экране. */
const EDITORS = {
  nano: [
    ["Сохранить ^O", "\x0f"],
    ["Выйти ^X", "\x18"],
    ["Да", "y"],
    ["Нет", "n"],
  ],
  vim: [
    ["Сохранить и выйти", `${ESC}:wq\r`],
    ["Выйти без сохранения", `${ESC}:q!\r`],
    ["Правка", "i"],
  ],
  less: [["Выйти q", "q"]],
};

/** Какой редактор открыт: по экрану терминала. */
export function editorOn(term) {
  const buffer = term.buffer.active;
  if (buffer.type !== "alternate") return "";
  const lines = [];
  for (let row = 0; row < term.rows; row++) lines.push(buffer.getLine(row)?.translateToString(true) ?? "");
  const text = lines.join("\n");
  if (/GNU nano|\^G Help|\^X Exit|\^G Справка|\^X Выход/.test(text)) return "nano";
  if (/-- (INSERT|ВСТАВКА|VISUAL) --/.test(text) || lines.filter((line) => line.startsWith("~")).length > 3) return "vim";
  if (/\(END\)|^:$/m.test(text)) return "less";
  return "";
}

/**
 * Плашка клавиш. `write` — отправить в терминал, `step` — команда проверки
 * шага. Отдаёт `{ el, refresh, withCtrl }`.
 */
export function keyBar({ term, write, step = () => "" }) {
  let ctrl = false;
  const bar = el("div", "tkeys");
  const extra = el("span", "tkeys__extra");
  bar.append(extra);

  const press = (node, action) => {
    // Фокус остаётся в терминале — клавиатура телефона не прячется.
    node.addEventListener("pointerdown", (event) => event.preventDefault());
    node.addEventListener("click", () => {
      action();
      term.focus();
    });
  };
  // Вставка: последний перевод строки не шлём — Enter за вами; в оболочке
  // строки вставки выполняются сразу, поэтому опасная среди них —
  // переспросить, как и набранную.
  const paste = async () => {
    const got = await api?.invoke("plugin:sufler|clipboard").catch(() => null);
    const text = String(got?.text ?? "").replace(/\r?\n$/, "");
    if (!text) return;
    const send = () => write(text.replace(/\r?\n/g, "\r"));
    const risky = editorOn(term) ? "" : text.split(/\r?\n/).map(danger).find(Boolean);
    if (risky) confirmDanger(text, risky, send);
    else send();
  };
  const ctrlKey = el("button", "tkey tkey--mod", "Ctrl");
  for (const [name, seq, title] of KEYS) {
    const key = seq === null ? ctrlKey : el("button", seq === "paste" ? "tkey tkey--mod" : "tkey", name);
    if (title) key.title = title;
    press(key, () => {
      if (seq === null) {
        ctrl = !ctrl;
        ctrlKey.setAttribute("aria-pressed", String(ctrl));
      } else if (seq === "paste") paste();
      else write(seq);
    });
    bar.append(key);
  }

  let shown = null;
  /** Кнопки редактора или проверка шага — в начале плашки. */
  const refresh = () => {
    const mode = editorOn(term);
    const check = mode ? "" : step();
    const now = `${mode}|${check}`;
    if (now === shown) return;
    shown = now;
    extra.replaceChildren();
    for (const [name, seq] of EDITORS[mode] ?? []) {
      const chip = el("button", "tkey tkey--editor", name);
      press(chip, () => write(seq));
      extra.append(chip);
    }
    if (check) {
      const chip = el("button", "tkey tkey--check", "проверка шага");
      chip.title = `Вписать «${check}» — выполнит ваш Enter`;
      press(chip, () => write(check));
      extra.append(chip);
    }
    bar.scrollLeft = 0;
  };
  refresh();

  /** Ctrl залип — следующая буква уходит управляющим знаком (Ctrl+C, Ctrl+X). */
  const withCtrl = (data) => {
    if (!ctrl || data.length !== 1 || !/[a-z@[\]\\^_]/i.test(data)) return data;
    ctrl = false;
    ctrlKey.setAttribute("aria-pressed", "false");
    return String.fromCharCode(data.toUpperCase().charCodeAt(0) & 0x1f);
  };
  return { el: bar, refresh, withCtrl };
}

/* ── Опасные команды ─────────────────────────────────────────────────── */

/** Что ломает сервер или отрезает от него — с объяснением, чем опасно. */
const DANGERS = [
  [/\brm\s+(-\w*\s+)*-\w*(rf|fr)\w*\s+(\/|\/\*|~|~\/|\*|\.\*?)(\s|$)/, "Удалит всё в корне, домашней папке или текущей папке — без корзины и без возврата."],
  [/\brm\s+(-\w*\s+)*--no-preserve-root/, "Удаление корня файловой системы — сервер перестанет работать."],
  [/\bmkfs(\.\w+)?\b/, "Форматирует диск или раздел — данные на нём пропадут."],
  [/\bdd\b.*\bof=\/dev\/(sd|nvme|vd|xvd|hd|mmcblk)/, "Пишет прямо на диск поверх данных — ошибка в имени диска уничтожит систему."],
  [/>\s*\/dev\/(sd|nvme|vd|xvd)/, "Перезапишет диск поверх данных."],
  [/:\(\)\s*\{\s*:\s*\|\s*:\s*&\s*\}\s*;\s*:/, "Форк-бомба: забьёт сервер процессами, поможет только перезагрузка."],
  [/\bchmod\s+(-\w+\s+)*-R\s+0?777\s+\/(\s|$)/, "Откроет всю систему на запись всем — сломает права и безопасность."],
  [/\bchown\s+(-\w+\s+)*-R\s+\S+\s+\/(\s|$)/, "Сменит владельца всей системы — сломает sudo, ssh и службы."],
  // Сама команда — в начале строки, после sudo или после ; && | — а не слово
  // внутри другой (grep shutdown, echo reboot).
  [/(^|[;&|]\s*|sudo\s+)(shutdown|poweroff|halt|reboot|init\s+[06])\b/, "Выключит или перезагрузит сервер — сеанс оборвётся, службы встанут."],
  [/\bsystemctl\s+(stop|disable|mask)\s+(sshd?|ssh\.service|sshd\.service)\b/, "Остановит SSH — вы потеряете доступ к серверу."],
  [/\bufw\s+(--force\s+)?enable\b/, "Включит фаервол: если SSH (порт 22) не разрешён заранее — `ufw allow OpenSSH`, — вы отрежете себя от сервера."],
  [/\biptables\s+(-P\s+INPUT\s+DROP|-F)\b/, "Правила фаервола: можно закрыть себе SSH и потерять доступ."],
  [/\b(curl|wget)\b[^|]*\|\s*(sudo\s+)?(ba|z)?sh\b/, "Скачанный скрипт выполнится сразу, не глядя. Сначала скачайте и прочитайте его."],
  [/\bpasswd\s+(-\w+\s+)*-d\b/, "Удалит пароль пользователя — вход станет без пароля."],
  [/\buserdel\s+(-\w+\s+)*-r\b/, "Удалит пользователя вместе с домашней папкой."],
  [/\bdocker\s+system\s+prune\s+(-\w+\s+)*-a/, "Удалит все неиспользуемые образы, сети и кэш сборки Docker."],
  [/\bdocker\s+volume\s+(rm|prune)\b/, "Удалит тома Docker — данные контейнеров пропадут."],
  [/\bkubectl\s+delete\s+(ns|namespace|nodes?|pv|pvc|--all)\b/, "Удалит пространство имён, узел или тома кластера вместе со всем, что в них."],
  [/\bgit\s+push\s+(.*\s)?(-f|--force)(\s|$)/, "Перезапишет историю на сервере — чужие коммиты пропадут."],
  [/\b(drop\s+(database|table)|truncate\s+table)\b/i, "Удалит базу или таблицу целиком."],
];

/** Чем опасна строка; пустая — безопасна. */
export function danger(line) {
  const command = String(line ?? "").trim();
  if (!command) return "";
  for (const [pattern, why] of DANGERS) if (pattern.test(command)) return why;
  return "";
}

/** Строка с курсором — без приглашения оболочки (`user@host:~$ `). */
export function typedLine(term) {
  const buffer = term.buffer.active;
  if (buffer.type === "alternate") return "";
  const line = buffer.getLine(buffer.baseY + buffer.cursorY)?.translateToString(true) ?? "";
  const prompt = Math.max(line.lastIndexOf("$ "), line.lastIndexOf("# "));
  return prompt >= 0 ? line.slice(prompt + 2) : line;
}

/** Опасная команда — переспросить. `go` — выполнить (отправить Enter). */
export function confirmDanger(command, why, go) {
  sheet("Осторожно", (box, close) => {
    box.append(el("span", "muted", why));
    const code = el("div", "step-card__check", command);
    box.append(code, el("span", "small dim", "Ноа остановила Enter. Если вы уверены — выполните; иначе отмените и поправьте команду."));
    const row = el("div", "pair");
    const cancel = el("button", "btn btn--secondary", "Отмена");
    cancel.addEventListener("click", close);
    const run = el("button", "btn btn--danger", "Выполнить");
    run.addEventListener("click", () => {
      close();
      go();
    });
    row.append(cancel, run);
    box.append(row);
  });
}
