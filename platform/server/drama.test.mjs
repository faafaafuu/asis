import { test } from "node:test";
import assert from "node:assert/strict";
import { checkEpisode, parseEpisode, rules } from "./drama.mjs";

const line = (id, speaker, spoken, delivery = "спокойно") => ({ id, speaker, delivery, spoken, display: spoken });
const many = (scene, from, count) =>
  Array.from({ length: count }, (_, i) => ({
    ...line(`l${from + i}`, ["junior", "lena", "palych"][i % 3], "Сайт лежит, клиенты звонят, а под всё перезапускается и перезапускается снова без конца."),
    ...(i % 6 === 0 ? { command: "kubectl get pods", output: "web-0  0/1  CrashLoopBackOff  11" } : {}),
  }));

function episode(extra = {}) {
  const s1 = { id: "s1", setting: "Ночь. Офис пуст. Звонит телефон.", lines: many("s1", 1, 25) };
  const s2 = { id: "s2", setting: "Утро. Кофе остыл.", lines: many("s2", 26, 25) };
  const ref = { scene: "s1", line: "l1" };
  const q = (type) => ({ type, q: "Что случилось?", options: ["Упал под", "Упал сервер"], answer: "Упал под", explain: "Вспомни звонок.", ref });
  return {
    title: "Под в цикле",
    new_terms: [{ term: "CrashLoopBackOff", spoken: "краш-луп-бэк-офф" }],
    episode: {
      title: "Ночной звонок",
      recap: "",
      hook: "",
      scenes: [s1, s2],
      quiz: [q("recall"), q("scenario"), q("compare"), q("recall"), q("scenario")],
      delayed_quiz: [q("recall"), q("compare"), q("scenario")].map(({ ref: _, ...rest }) => rest),
      ...extra,
    },
  };
}

test("годная серия проходит проверку", () => {
  assert.deepEqual(checkEpisode(episode(), { n: 1, total: 1, minutes: 5 }), []);
});

test("латиница и цифры в произносимом — ошибка, в display — можно", () => {
  const bad = episode();
  bad.episode.scenes[0].lines[0].spoken = "Проверь порт 443 через nc";
  const problems = checkEpisode(bad, { minutes: 5 });
  assert.ok(problems.some((p) => p.includes("l1") && p.includes("латиница")));
});

test("голоса, подача, ссылки вопросов и повтор через день", () => {
  const bad = episode();
  bad.episode.scenes[0].lines[1].speaker = "boss";
  bad.episode.scenes[0].lines[2].delivery = "громко";
  bad.episode.quiz[0].ref = { scene: "s9", line: "l99" };
  bad.episode.delayed_quiz = [];
  const problems = checkEpisode(bad, { minutes: 5 }).join("\n");
  assert.match(problems, /speaker «boss»/);
  assert.match(problems, /delivery «громко»/);
  assert.match(problems, /нет такой реплики/);
  assert.match(problems, /delayed_quiz/);
});

test("сериал: со второй серии — «в прошлой серии», не последняя — крючок", () => {
  const problems = checkEpisode(episode(), { n: 2, total: 3, minutes: 5 }).join("\n");
  assert.match(problems, /recap/);
  assert.match(problems, /hook/);
});

test("ответ модели с текстом вокруг разбирается", () => {
  const parsed = parseEpisode(`Вот серия:\n\`\`\`json\n${JSON.stringify(episode())}\n\`\`\``);
  assert.equal(parsed.episode.title, "Ночной звонок");
});

test("указания называют имя джуна и длину", () => {
  const text = rules({ level: "junior", junior: "Вася", minutes: 3, scope: "lesson", total: 1 });
  assert.match(text, /Вася/);
  assert.match(text, /около 3 минут/);
});

test("разбор без команд не проходит: герои должны вводить и смотреть", () => {
  const bare = episode();
  for (const scene of bare.episode.scenes) for (const l of scene.lines) delete l.command;
  assert.match(checkEpisode(bare, { minutes: 5 }).join("\n"), /команд в разборе 0/);
});
