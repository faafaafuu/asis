// Описание релиза для GitHub: «что нового» из CHANGELOG.md плюс постоянная часть
// про установку.
//
// Зачем скриптом, а не текстом прямо в workflow: одинаковый шаблон в каждом релизе
// не отвечает на единственный вопрос, с которым туда приходят, — «что изменилось».
// А держать заметки в двух местах (файл проекта и настройки сборки) — верный способ
// получить два разных ответа.
//
// Запуск: node scripts/release-notes.mjs v0.1.6

import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");

/** Раздел CHANGELOG.md для версии. Заголовки имеют вид `## 0.1.6 — про что версия`. */
export function sectionFor(changelog, version) {
  const lines = changelog.split("\n");
  // Начало — заголовок ровно этой версии. Сравниваем по первому слову после `## `,
  // иначе 0.1.1 совпало бы с 0.1.10.
  const start = lines.findIndex((line) => {
    const match = /^##\s+(\S+)/.exec(line);
    return match?.[1] === version;
  });
  if (start < 0) return "";

  const rest = lines.slice(start + 1);
  const end = rest.findIndex((line) => line.startsWith("## "));
  const body = (end < 0 ? rest : rest.slice(0, end)).join("\n").trim();
  return body;
}

const INSTALL = `## Скачать

Пока только **Windows 10 / 11** — файл \`.exe\`. Скачайте и запустите, командная
строка не нужна. Сборки под macOS, Linux и Android появятся, когда заработают
и будут проверены: обещать файл, которого нет, хуже, чем честно его не предлагать.

**При первом запуске** Windows покажет «Windows защитила ваш компьютер» →
«Подробнее» → «Выполнить в любом случае». Так система встречает любую программу
без платной подписи разработчика.

**Как пользоваться.** Выделите текст в любой программе, удерживая **левый Ctrl** —
рядом появится объяснение. Программа живёт в трее.`;

export function releaseNotes(changelog, tag) {
  const version = tag.replace(/^v/, "");
  const section = sectionFor(changelog, version);
  // Без раздела релиз всё равно должен быть полезен: лучше одна install-часть,
  // чем сборка, упавшая из-за забытой строчки в CHANGELOG.
  const whatsNew = section ? `## Что нового\n\n${section}\n\n` : "";
  return `${whatsNew}${INSTALL}`;
}

const tag = process.argv[2];
if (tag) {
  const changelog = readFileSync(join(root, "CHANGELOG.md"), "utf8");
  process.stdout.write(releaseNotes(changelog, tag));
}
