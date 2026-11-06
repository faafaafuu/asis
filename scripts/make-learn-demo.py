"""Демо-набор витрины приложения обучения — src/dev/learn-demo.json.

Выдуманные курсы («Астрономия с нуля», «Основы фотографии», «Python для
анализа данных») в том же виде, что выгрузка learning::dump_learn_fixtures.
По нему снимаются экраны для сайта и README: в публичное не попадают
настоящие курсы, цели и прогресс владельца. Витрина берёт его по ?demo=1.

Запуск: python scripts/make-learn-demo.py
"""

import json
import os

HERE = os.path.dirname(os.path.abspath(__file__))
OUT = os.path.join(HERE, "..", "src", "dev", "learn-demo.json")

ASTRO = [
    ("sky", "Небо над головой", "Созвездия, суточное движение неба, как найти Полярную звезду.",
     [("constellation", "Созвездие", "Участок неба с условной фигурой из ярких звёзд; всего их 88."),
      ("pole-star", "Полярная звезда", "Звезда почти над северным полюсом мира — по ней находят север."),
      ("ecliptic", "Эклиптика", "Путь Солнца по небу за год; вдоль неё ходят и планеты."),
      ("magnitude", "Звёздная величина", "Мера блеска: чем меньше число, тем ярче звезда.")]),
    ("light", "Свет и спектр", "Почему звёзды разного цвета и что спектр рассказывает о далёком объекте.",
     [("spectrum", "Спектр", "Свет, разложенный по длинам волн; линии в нём — подпись вещества."),
      ("doppler", "Эффект Доплера", "Сдвиг линий спектра: к красному — объект удаляется, к синему — приближается."),
      ("wavelength", "Длина волны", "Расстояние между гребнями волны; задаёт цвет видимого света."),
      ("blackbody", "Чёрное тело", "Тело, излучающее только из-за своей температуры: горячее — голубее.")]),
    ("albedo", "Альбедо и отражение", "Почему Луна кажется яркой, хотя тёмная, и как свет говорит о поверхности планет.",
     [("albedo", "Альбедо", "Доля падающего света, которую поверхность отражает: 0 — всё поглощает, 1 — всё отражает."),
      ("phase", "Фаза", "Освещённая Солнцем часть диска, видимая с Земли."),
      ("opposition", "Противостояние", "Планета напротив Солнца: ближе всего и ярче всего."),
      ("regolith", "Реголит", "Рыхлый слой пыли и обломков на поверхности Луны и астероидов."),
      ("greenhouse", "Парниковый эффект", "Атмосфера пропускает свет, но удерживает тепловое излучение.")]),
    ("sun", "Солнце", "Ближайшая звезда: из чего состоит, откуда берётся энергия, что такое пятна.",
     [("fusion", "Термоядерный синтез", "Слияние ядер водорода в гелий в ядре звезды — источник её энергии."),
      ("sunspot", "Солнечное пятно", "Тёмная холодная область на поверхности с сильным магнитным полем."),
      ("corona", "Корона", "Внешняя горячая атмосфера Солнца, видна во время полного затмения.")]),
    ("rocky", "Планеты земной группы", "Меркурий, Венера, Земля и Марс: чем похожи и почему такие разные.",
     [("terrestrial", "Планета земной группы", "Каменная планета с твёрдой поверхностью."),
      ("crater", "Кратер", "Чаша от удара метеорита; много кратеров — старая поверхность.")]),
    ("giants", "Газовые гиганты", "Юпитер и Сатурн: полосы, кольца и спутники.",
     [("ring", "Кольца", "Облако льда и пыли на орбите вокруг планеты."),
      ("moon", "Спутник", "Тело, которое обращается вокруг планеты.")]),
    ("stars", "Жизнь звёзд", "От облака газа до белого карлика, нейтронной звезды или чёрной дыры.",
     [("nebula", "Туманность", "Облако газа и пыли, из которого рождаются звёзды."),
      ("supernova", "Сверхновая", "Взрыв массивной звезды в конце жизни.")]),
    ("galaxies", "Галактики", "Млечный Путь и соседи: спирали, эллипсы и расширение Вселенной.",
     [("galaxy", "Галактика", "Система из миллиардов звёзд, газа и тёмной материи."),
      ("redshift", "Красное смещение", "Сдвиг спектра далёких галактик — признак расширения Вселенной.")]),
]

PHOTO = [("light-photo", "Свет", "Жёсткий и мягкий свет, направление, золотой час."),
         ("exposure", "Экспозиция", "Выдержка, диафрагма и ISO — треугольник экспозиции."),
         ("composition", "Композиция", "Правило третей, линии, передний план."),
         ("focus", "Резкость и глубина", "Где фокус и почему фон размыт."),
         ("color", "Цвет", "Баланс белого и настроение кадра."),
         ("editing", "Обработка", "Кадрирование, кривые, бережная ретушь.")]

PYDATA = [("basics", "Python за вечер", "Переменные, списки, циклы, функции."),
          ("pandas", "Таблицы в pandas", "Загрузить, отфильтровать, сгруппировать."),
          ("plots", "Графики", "Matplotlib: линия, столбцы, гистограмма."),
          ("stats", "Описательная статистика", "Среднее, медиана, разброс."),
          ("clean", "Чистка данных", "Пропуски, дубли, типы."),
          ("join", "Объединение таблиц", "merge и join без потерь."),
          ("report", "Отчёт", "Вывод в понятный отчёт.")]


def lesson(title, summary, concepts):
    parts = [f"{summary}"]
    for cid, term, definition in concepts:
        parts.append(
            f"## {term}\n\n**{term}** — {definition[0].lower() + definition[1:]}\n\n"
            f"Представьте, что смотрите на ночное небо из тёмного места за городом: {term.lower()} — то, что помогает понять увиденное. "
            "Сначала заметьте явление, потом назовите его, потом проверьте себя: объясните другу в двух фразах."
        )
    return "\n\n".join(parts)


def concept(cid, term, definition, others):
    return {
        "id": cid,
        "term": term,
        "definition": definition,
        "mnemonic": "",
        "analogy": "Как свет фонарика на снегу и на асфальте." if cid == "albedo" else "",
        "example": "Альбедо свежего снега — около 0,9, Луны — около 0,12." if cid == "albedo" else "",
        "pitfall": "Яркость на небе зависит не только от альбедо, но и от расстояния и освещения." if cid == "albedo" else "",
        "related": [o for o in others if o != cid][:2],
    }


def topic_view(tid, title, summary, concepts):
    ids = [c[0] for c in concepts]
    cs = [concept(cid, term, d, ids) for cid, term, d in concepts]
    tasks = [
        {"id": f"{tid}-t1", "kind": "open", "q": f"Объясните своими словами: {concepts[0][1].lower()}.", "options": [], "answer": None, "explain": "",
         "points": [concepts[0][2]], "reference": concepts[0][2], "concept": concepts[0][0]},
        {"id": f"{tid}-t2", "kind": "choice", "q": f"Что из этого — {concepts[-1][1].lower()}?", "options": [concepts[-1][2], concepts[0][2]], "answer": 0,
         "explain": "Первое — определение, второе — другое понятие темы.", "points": [], "reference": "", "concept": concepts[-1][0]},
    ]
    exam = [
        {"id": f"{tid}-e{i + 1}", "kind": "open", "q": f"Чем важно понятие «{term}»?", "options": [], "answer": None, "explain": "",
         "points": [definition], "reference": definition, "concept": cid}
        for i, (cid, term, definition) in enumerate(concepts[:3])
    ]
    return {
        "topic": {"id": tid, "title": title, "summary": summary, "aliases": [], "lesson": lesson(title, summary, concepts),
                  "concepts": cs, "cards": [], "tasks": tasks, "exam": exam, "cheatsheet": ""},
        "cheatsheet": "\n".join(f"- **{term}** — {d}" for _, term, d in concepts),
        "mistakes": [],
        "scores": {},
    }


def ov_topic(tid, title, summary, n, state):
    read, status, exam_best, step, section = {
        "done": (True, "done", 85, "map", 0),
        "now": (False, "new", None, "lesson", 1),
        "new": (False, "new", None, "lesson", 0),
    }[state]
    return {"id": tid, "title": title, "summary": summary, "status": status, "read": read, "tasksDone": 2 if state == "done" else 0,
            "tasksTotal": 2, "examBest": exam_best, "mistakes": 0, "conceptsTotal": n, "conceptsMature": 1 if state == "done" else 0,
            "step": step, "section": section}


def course(cid, title, description, topics, done, current, percent, due, focus):
    return {
        "id": cid, "title": title, "description": description, "percent": percent, "finalBest": None, "finalUnlocked": False,
        "current": current, "topicPass": 70, "finalPass": 75,
        "mastery": {"total": sum(t["conceptsTotal"] for t in topics), "new": 12, "learning": 6, "young": 3, "mature": 1, "due": due, "newLeft": 10},
        "focus": focus, "topics": topics,
    }


astro_topics = [ov_topic(t, title, s, len(c), "done" if i < 2 else "now" if i == 2 else "new") for i, (t, title, s, c) in enumerate(ASTRO)]
photo_topics = [ov_topic(t, title, s, 4, "done" if i < 2 else "now" if i == 2 else "new") for i, (t, title, s) in enumerate(PHOTO)]
py_topics = [ov_topic(t, title, s, 4, "now" if i == 0 else "new") for i, (t, title, s) in enumerate(PYDATA)]
focus = {"today": 15, "week": 95, "streak": 4, "sessionsToday": 1}

data = {
    "overview": [
        course("astro", "Астрономия с нуля", "Для тех, кто хочет понимать, что видит на ночном небе: свет, планеты, звёзды и галактики — без формул там, где можно без них.",
               astro_topics, 2, "albedo", 28, 18, focus),
        course("photo", "Основы фотографии", "Свет, экспозиция и композиция — чтобы снимать осознанно, а не наугад.", photo_topics, 2, "composition", 34, 6, focus),
        course("pydata", "Python для анализа данных", "От первой строки кода до понятного отчёта по таблице.", py_topics, 0, "basics", 0, 0, focus),
    ],
    "topics": {f"astro/{t}": topic_view(t, title, s, c) for t, title, s, c in ASTRO},
    "exams": {f"astro/{t}": {"pass": 70, "questions": topic_view(t, title, s, c)["topic"]["exam"]} for t, title, s, c in ASTRO},
    "reviews": {},
    "maps": {
        "astro": {
            "topics": [{"id": t, "title": title, "concepts": [{"id": cid, "term": term, "level": "young" if i < 2 else "learning" if i == 2 else "new"} for cid, term, _ in c]}
                       for i, (t, title, _, c) in enumerate(ASTRO)],
            "edges": [{"from": "albedo/albedo", "to": "albedo/phase", "cross": False}, {"from": "light/spectrum", "to": "albedo/albedo", "cross": True}],
        }
    },
    "answers": {},
}

cards = []
for i, (t, title, _, c) in enumerate(ASTRO[:3]):
    for cid, term, definition in c:
        cards.append({"key": f"{t}/c-{cid}", "topic": t, "topicTitle": title, "concept": cid, "front": f"Что такое «{term}»?", "back": definition,
                      "level": "learning", "fresh": i == 2, "mnemonic": "", "analogy": "", "example": "", "pitfall": ""})
data["reviews"]["astro"] = cards
for t, *_ in ASTRO[:3]:
    data["reviews"][f"astro/{t}"] = [c for c in cards if c["topic"] == t]

with open(OUT, "w", encoding="utf-8") as f:
    json.dump(data, f, ensure_ascii=False, indent=1)
print("ok", OUT)
