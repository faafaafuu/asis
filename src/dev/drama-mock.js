// Витрина аудиодрамы: одна озвученная сцена (звук — src/dev/handoff, в git не идёт).
const timeline = [
  { scene: "s1", line: "setting", speaker: "narrator", start: 0.0, end: 3.32 },
  { scene: "s1", line: "l1", speaker: "junior", start: 3.92, end: 8.55 },
  { scene: "s1", line: "l2", speaker: "lena", start: 8.83, end: 16.66 },
  { scene: "s1", line: "l3", speaker: "palych", start: 16.94, end: 22.69 },
  { scene: "s1", line: "l4", speaker: "junior", start: 22.97, end: 25.29 },
  { scene: "s1", line: "l5", speaker: "lena", start: 25.74, end: 30.37 },
  { scene: "", line: "hook", speaker: "narrator", start: 31.47, end: 34.74 },
];
const ref = { scene: "s1", line: "l3" };
const script = {
  title: "Под в цикле",
  cast: [{ id: "junior", name: "Костя" }],
  new_terms: [{ term: "CrashLoopBackOff", spoken: "краш-луп-бэк-офф" }],
  episodes: [
    {
      n: 1,
      title: "Ночной звонок",
      ready: true,
      timeline,
      recap: "",
      hook: "А утром Палыч нашёл в логах кое-что похуже.",
      scenes: [
        {
          id: "s1",
          setting: "Ночь. Офис пуст. Звонит телефон.",
          lines: [
            { id: "l1", speaker: "junior", display: "Лена! Сайт лежит, клиенты звонят, а pod перезапускается каждые 10 секунд!" },
            { id: "l2", speaker: "lena", display: "Спокойно, Костя. Это CrashLoopBackOff: под падает, Kubernetes поднимает его снова и ждёт всё дольше." },
            { id: "l3", speaker: "palych", display: "Заповедь первая: сначала логи, потом паника." },
            { id: "l4", speaker: "junior", display: "А где их смотреть, если он уже умер?" },
            { id: "l5", speaker: "lena", display: "kubectl logs --previous — логи прошлого запуска." },
          ],
        },
      ],
      quiz: [
        { type: "recall", q: "Что Палыч велел сделать первым?", options: ["Перезапустить под", "Посмотреть логи", "Позвонить клиентам"], answer: "Посмотреть логи", explain: "Его заповедь: сначала логи, потом паника.", ref },
        { type: "scenario", q: "Под уже упал и перезапустился. Как увидеть, что он сказал перед падением?", options: ["kubectl logs --previous", "kubectl get pods", "kubectl delete pod"], answer: "kubectl logs --previous", explain: "Лена: логи прошлого запуска.", ref: { scene: "s1", line: "l5" } },
      ],
      delayed_quiz: [],
    },
  ],
};

export function mockDrama(cmd) {
  if (cmd === "learn_dramas") return { dramas: [{ id: "d1", scope: "lesson", status: "done", ready: 1, total: 1, title: "Под в цикле", created: "2026-10-21 18:00:00" }] };
  if (cmd === "learn_drama_script") return script;
  if (cmd === "learn_drama_file") return "/dev/handoff/drama-test.mp3";
  if (cmd === "learn_drama_status") return { id: "d1", scope: "lesson", status: "done", ready: 1, total: 1 };
  return undefined;
}
