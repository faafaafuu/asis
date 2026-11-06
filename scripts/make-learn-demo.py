"""Демо-набор витрины приложения обучения — src/dev/learn-demo.json.

Выдуманные, но настоящие по содержанию курсы: «Матанализ к сессии»,
«Органическая химия: механизмы реакций», «Собеседование: системный
аналитик» — в том же виде, что выгрузка learning::dump_learn_fixtures. По
нему снимаются экраны для сайта и README: в публичное не попадают курсы,
цели и прогресс владельца. Витрина берёт его по ?demo=1.

Запуск: python scripts/make-learn-demo.py
"""

import json
import os

HERE = os.path.dirname(os.path.abspath(__file__))
OUT = os.path.join(HERE, "..", "src", "dev", "learn-demo.json")

# Тема: id, название, о чём, понятия (id, термин, определение), разделы урока (заголовок, текст).
MATAN = [
    ("limits", "Пределы последовательностей", "Что значит «стремится», ε–N без страха и теорема Больцано–Вейерштрасса.",
     [("eps-n", "Предел по ε–N", "Число a — предел xₙ, если для любого ε > 0 найдётся N, что |xₙ − a| < ε при всех n > N."),
      ("cauchy-seq", "Фундаментальная последовательность", "Последовательность, члены которой с некоторого номера сколь угодно близки друг к другу; в ℝ она сходится."),
      ("bolzano", "Теорема Больцано–Вейерштрасса", "Из любой ограниченной последовательности можно выделить сходящуюся подпоследовательность.")], []),
    ("continuity", "Предел функции и непрерывность", "ε–δ, замечательные пределы и что ломается в точке разрыва.",
     [("eps-delta", "Предел по Коши (ε–δ)", "lim f(x) = A при x → a, если для любого ε > 0 есть δ > 0: 0 < |x − a| < δ ⇒ |f(x) − A| < ε."),
      ("continuous", "Непрерывность в точке", "f непрерывна в a, если предел f(x) при x → a равен f(a)."),
      ("weierstrass", "Теорема Вейерштрасса", "Непрерывная на отрезке функция ограничена и достигает наибольшего и наименьшего значений.")], []),
    ("derivative", "Производная и дифференциал", "Скорость изменения, касательная, правила дифференцирования и Лопиталь.",
     [("derivative", "Производная", "Предел отношения приращения функции к приращению аргумента: f′(a) = lim (f(a+h) − f(a)) / h."),
      ("differential", "Дифференциал", "Главная линейная часть приращения: df = f′(x)·dx."),
      ("lhopital", "Правило Лопиталя", "Предел отношения в неопределённости 0/0 или ∞/∞ равен пределу отношения производных, если тот существует.")], []),
    ("mean-value", "Теоремы о среднем", "Ролль, Лагранж и Коши — зачем они и как их применять в задачах.",
     [("rolle", "Теорема Ролля", "Если f непрерывна на [a, b], дифференцируема на (a, b) и f(a) = f(b), то есть c с f′(c) = 0."),
      ("lagrange", "Теорема Лагранжа", "f(b) − f(a) = f′(c)(b − a) для некоторой c из (a, b): средняя скорость где-то достигается мгновенной.")], []),
    ("taylor", "Формула Тейлора", "Как приблизить любую гладкую функцию многочленом и оценить, насколько ошиблись.",
     [("taylor", "Формула Тейлора", "Представление f в окрестности a многочленом степени n: f(x) = Σ f⁽ᵏ⁾(a)/k! · (x − a)ᵏ + Rₙ(x)."),
      ("peano", "Остаток в форме Пеано", "Rₙ(x) = o((x − a)ⁿ) при x → a: ошибка мала по сравнению с последним учтённым слагаемым."),
      ("lagrange-rest", "Остаток в форме Лагранжа", "Rₙ(x) = f⁽ⁿ⁺¹⁾(ξ)/(n+1)! · (x − a)ⁿ⁺¹ для некоторой ξ между a и x — даёт оценку ошибки числом."),
      ("maclaurin", "Ряд Маклорена", "Формула Тейлора в точке a = 0: eˣ = 1 + x + x²/2! + …, sin x = x − x³/3! + …"),
      ("small-o", "o-малое", "α(x) = o(β(x)), если α/β → 0: α исчезает быстрее β.")],
     [("Зачем приближать многочленом",
       "Многочлен считается одними сложениями и умножениями — так калькулятор и компьютер находят sin x, eˣ и ln x. Идея: подобрать многочлен, у которого в точке a совпадают с функцией значение и первые n производных. Чем больше совпавших производных, тем дольше многочлен «держится» рядом с графиком."),
      ("Остаточный член — сколько мы ошиблись",
       "**Остаток в форме Лагранжа** Rₙ(x) = f⁽ⁿ⁺¹⁾(ξ)/(n+1)! · (x − a)ⁿ⁺¹ отвечает на главный вопрос экзамена: насколько многочлен отличается от функции. Точку ξ мы не знаем, но знаем, где она лежит, — и этого хватает, чтобы оценить ошибку сверху.\n\nПример: оценить ошибку e ≈ 1 + 1 + 1/2 + 1/6. Здесь n = 3, f⁽⁴⁾(ξ) = e^ξ < 3, значит R₃ < 3/4! = 0,125.\n\n```\neˣ = 1 + x + x²/2! + x³/3! + R₃(x),   |R₃(1)| < 3/24\n```"),
      ("Пеано: когда нужен предел, а не оценка",
       "**Остаток в форме Пеано** o((x − a)ⁿ) удобен в пределах: sin x = x − x³/6 + o(x³) превращает «страшный» предел (sin x − x)/x³ в −1/6 за одну строку."),
      ("Частые ошибки на экзамене",
       "> Частая ошибка: путать, где нужен Лагранж, а где Пеано. Оценка ошибки числом — Лагранж; вычисление предела — Пеано.\n\nЕщё: забывают факториал в знаменателе и раскладывают не до того порядка, который нужен в пределе.")]),
    ("study", "Исследование функции", "Монотонность, экстремумы, выпуклость, асимптоты — полный план графика.",
     [("extremum", "Экстремум", "Точка, где функция принимает наибольшее или наименьшее значение среди близких точек."),
      ("convex", "Выпуклость", "График лежит ниже хорд на промежутке; признак — f″ ≤ 0.")], []),
    ("indefinite", "Неопределённый интеграл", "Первообразные, замена переменной и интегрирование по частям.",
     [("antiderivative", "Первообразная", "F такая, что F′ = f; все первообразные отличаются на константу."),
      ("by-parts", "Интегрирование по частям", "∫u dv = uv − ∫v du.")], []),
    ("definite", "Определённый интеграл", "Площадь, суммы Римана и формула Ньютона–Лейбница.",
     [("riemann", "Интеграл Римана", "Предел интегральных сумм при измельчении разбиения."),
      ("newton-leibniz", "Формула Ньютона–Лейбница", "∫ₐᵇ f(x) dx = F(b) − F(a), где F — первообразная f.")], []),
]

CHEM = [("bonds", "Связи и гибридизация", "sp, sp², sp³ и почему метан — тетраэдр."),
        ("effects", "Электронные эффекты", "Индуктивный и мезомерный эффекты: кто тянет электроны."),
        ("sn", "Нуклеофильное замещение", "SN1 и SN2: стереохимия, растворитель, субстрат."),
        ("elimination", "Отщепление E1 и E2", "Правило Зайцева и конкуренция с замещением."),
        ("addition", "Присоединение к алкенам", "Марковников, карбокатионы и перегруппировки."),
        ("aromatic", "Ароматическое замещение", "Ориентанты первого и второго рода.")]

ANALYST = [("requirements", "Требования", "Бизнес-, пользовательские и системные — как не перепутать на собеседовании."),
           ("uml", "UML и BPMN", "Диаграммы, которые просят нарисовать на доске."),
           ("sql", "SQL для аналитика", "JOIN, GROUP BY, оконные функции на живых задачах."),
           ("api", "REST и интеграции", "Методы, коды ответов, идемпотентность."),
           ("cases", "Кейсы", "Разбор типовых кейсов с собеседований."),
           ("soft", "Вопросы о себе", "Как рассказать об опыте коротко и по делу.")]


def lesson_text(title, summary, concepts, sections):
    if sections:
        return summary + "\n\n" + "\n\n".join(f"## {head}\n\n{body}" for head, body in sections)
    parts = [summary]
    for _, term, definition in concepts:
        parts.append(f"## {term}\n\n**{term}** — {definition[0].lower() + definition[1:]}\n\nРазберите на примере из задачника и проговорите своими словами — так определение держится до экзамена.")
    return "\n\n".join(parts)


def concept(cid, term, definition, others):
    extra = {
        "lagrange-rest": ("Как погрешность измерения: точное значение неизвестно, но известно, в каких пределах.", "Оценить ошибку e ≈ 2,667: R₃ < 3/24.", "Не путать с остатком Пеано: Лагранж даёт число, Пеано — порядок малости."),
        "eps-delta": ("Как допуск на чертеже: назови точность ε — я назову, насколько близко подойти δ.", "lim (2x + 1) = 3 при x → 1: δ = ε/2.", "δ зависит от ε, а не наоборот."),
        "taylor": ("Как фоторобот: чем больше черт совпало, тем точнее портрет.", "sin x ≈ x − x³/6 при малых x.", "Забывают факториал в знаменателе."),
    }.get(cid, ("", "", ""))
    return {"id": cid, "term": term, "definition": definition, "mnemonic": "", "analogy": extra[0], "example": extra[1], "pitfall": extra[2],
            "related": [o for o in others if o != cid][:2]}


def topic_view(tid, title, summary, concepts, sections):
    ids = [c[0] for c in concepts]
    cs = [concept(cid, term, d, ids) for cid, term, d in concepts]
    tasks = [
        {"id": f"{tid}-t1", "kind": "open", "q": f"Объясните своими словами: {concepts[0][1].lower()}. Приведите пример.", "options": [], "answer": None, "explain": "",
         "points": [concepts[0][2]], "reference": concepts[0][2], "concept": concepts[0][0]},
        {"id": f"{tid}-t2", "kind": "choice", "q": f"Какое утверждение верно описывает «{concepts[-1][1]}»?", "options": [concepts[-1][2], concepts[0][2]], "answer": 0,
         "explain": "Второй вариант — определение другого понятия темы.", "points": [], "reference": "", "concept": concepts[-1][0]},
    ]
    exam = [
        {"id": f"{tid}-e{i + 1}", "kind": "open", "q": f"Сформулируйте и поясните: {term}.", "options": [], "answer": None, "explain": "",
         "points": [definition], "reference": definition, "concept": cid}
        for i, (cid, term, definition) in enumerate(concepts[:3])
    ]
    return {
        "topic": {"id": tid, "title": title, "summary": summary, "aliases": [], "lesson": lesson_text(title, summary, concepts, sections),
                  "concepts": cs, "cards": [], "tasks": tasks, "exam": exam, "cheatsheet": ""},
        "cheatsheet": "\n".join(f"- **{term}** — {d}" for _, term, d in concepts),
        "mistakes": [],
        "scores": {},
    }


def ov_topic(tid, title, summary, n, state, mistakes=0):
    read, status, exam_best, step, section = {
        "done": (True, "done", 88, "map", 0),
        "now": (False, "new", None, "lesson", 1),
        "new": (False, "new", None, "lesson", 0),
    }[state]
    return {"id": tid, "title": title, "summary": summary, "status": status, "read": read, "tasksDone": 2 if state == "done" else 0,
            "tasksTotal": 2, "examBest": exam_best, "mistakes": mistakes, "conceptsTotal": n, "conceptsMature": 2 if state == "done" else 0,
            "step": step, "section": section}


def course(cid, title, description, topics, current, percent, due, focus):
    return {
        "id": cid, "title": title, "description": description, "percent": percent, "finalBest": None, "finalUnlocked": False,
        "current": current, "topicPass": 70, "finalPass": 75,
        "mastery": {"total": sum(t["conceptsTotal"] for t in topics), "new": 14, "learning": 9, "young": 6, "mature": 3, "due": due, "newLeft": 10},
        "focus": focus, "topics": topics,
    }


NOW = 4  # «Формула Тейлора» — пятая тема, сейчас.
matan_topics = [ov_topic(t, title, s, len(c), "done" if i < NOW else "now" if i == NOW else "new", mistakes={1: 3, 3: 2}.get(i, 0))
                for i, (t, title, s, c, _) in enumerate(MATAN)]
chem_topics = [ov_topic(t, title, s, 6, "done" if i < 2 else "now" if i == 2 else "new") for i, (t, title, s) in enumerate(CHEM)]
analyst_topics = [ov_topic(t, title, s, 5, "done" if i < 1 else "now" if i == 1 else "new") for i, (t, title, s) in enumerate(ANALYST)]
focus = {"today": 35, "week": 240, "streak": 9, "sessionsToday": 2}

data = {
    "overview": [
        course("matan", "Матанализ к сессии",
               "Первый курс, первый семестр: пределы, производные, формула Тейлора и интегралы — с доказательствами, которые спрашивают на устном экзамене.",
               matan_topics, "taylor", 52, 24, focus),
        course("orgchem", "Органическая химия: механизмы реакций", "SN1, SN2, E1, E2 и присоединение — механизмы стрелками, а не зубрёжкой.", chem_topics, "sn", 38, 9, focus),
        course("analyst", "Собеседование: системный аналитик", "Требования, UML, SQL, интеграции и кейсы — как спрашивают на собеседованиях.", analyst_topics, "uml", 21, 5, focus),
    ],
    "topics": {f"matan/{t}": topic_view(t, title, s, c, sec) for t, title, s, c, sec in MATAN},
    "exams": {f"matan/{t}": {"pass": 70, "questions": topic_view(t, title, s, c, sec)["topic"]["exam"]} for t, title, s, c, sec in MATAN},
    "reviews": {},
    "maps": {
        "matan": {
            "topics": [{"id": t, "title": title, "concepts": [{"id": cid, "term": term, "level": "mature" if i < 2 else "young" if i < NOW else "learning" if i == NOW else "new"} for cid, term, _ in c]}
                       for i, (t, title, _, c, _) in enumerate(MATAN)],
            "edges": [{"from": "derivative/derivative", "to": "taylor/taylor", "cross": True}, {"from": "mean-value/lagrange", "to": "taylor/lagrange-rest", "cross": True}],
        }
    },
    "answers": {},
}

cards = []
for i, (t, title, _, c, _) in enumerate(MATAN[: NOW + 1]):
    for cid, term, definition in c:
        cards.append({"key": f"{t}/c-{cid}", "topic": t, "topicTitle": title, "concept": cid, "front": f"Сформулируйте: {term}", "back": definition,
                      "level": "learning", "fresh": i == NOW, "mnemonic": "", "analogy": "", "example": "", "pitfall": ""})
cards.sort(key=lambda card: card["topic"] != "taylor")
data["reviews"]["matan"] = cards
for t, *_ in MATAN[: NOW + 1]:
    data["reviews"][f"matan/{t}"] = [c for c in cards if c["topic"] == t]

with open(OUT, "w", encoding="utf-8") as f:
    json.dump(data, f, ensure_ascii=False, indent=1)
print("ok", OUT)
