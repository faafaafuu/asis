import { test } from "node:test";
import assert from "node:assert/strict";
import { parseRich, parseInline, isRich } from "./rich-text.js";

test("обычный ответ — один абзац с переносами строк", () => {
  assert.deepEqual(parseRich("Первая строка.\nВторая строка."), [{ type: "p", text: "Первая строка.\nВторая строка." }]);
  assert.equal(isRich("Просто текст без разметки."), false);
});

test("абзацы разделяет пустая строка", () => {
  assert.deepEqual(
    parseRich("Раз.\n\nДва."),
    [
      { type: "p", text: "Раз." },
      { type: "p", text: "Два." },
    ],
  );
});

test("блок кода с языком и без", () => {
  assert.deepEqual(parseRich("Команда:\n```cmd\nping -n 4 ya.ru\n```"), [
    { type: "p", text: "Команда:" },
    { type: "code", lang: "cmd", text: "ping -n 4 ya.ru" },
  ]);
  assert.deepEqual(parseRich("```\na\nb\n```"), [{ type: "code", lang: "", text: "a\nb" }]);
});

test("незакрытый блок кода забирает текст до конца", () => {
  assert.deepEqual(parseRich("```js\nconst a = 1;"), [{ type: "code", lang: "js", text: "const a = 1;" }]);
});

test("таблица: шапка и строки", () => {
  const blocks = parseRich("| Видите | Значит |\n|---|---|\n| время=12мс | связь есть |\n| превышен интервал | адрес молчит |");
  assert.deepEqual(blocks, [
    {
      type: "table",
      head: ["Видите", "Значит"],
      rows: [
        ["время=12мс", "связь есть"],
        ["превышен интервал", "адрес молчит"],
      ],
    },
  ]);
});

test("строки с чертой без разделителя — не таблица", () => {
  assert.equal(parseRich("| просто текст |")[0].type, "p");
});

test("списки: маркированный и нумерованный с номера", () => {
  assert.deepEqual(parseRich("- раз\n- два"), [{ type: "ul", items: ["раз", "два"] }]);
  assert.deepEqual(parseRich("3. третий\n4. четвёртый"), [{ type: "ol", start: 3, items: ["третий", "четвёртый"] }]);
});

test("жирный и код в строке", () => {
  assert.deepEqual(parseInline("**Гасящая.** Термостат `t=21`"), [
    { t: "b", v: "Гасящая." },
    { t: "text", v: " Термостат " },
    { t: "code", v: "t=21" },
  ]);
  assert.equal(isRich("**Важно:** проверьте"), true);
});

test("заголовок — жирной строкой", () => {
  assert.deepEqual(parseRich("## Что делать\nПерезапустите роутер."), [{ type: "p", text: "**Что делать**\nПерезапустите роутер." }]);
});
