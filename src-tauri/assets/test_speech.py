"""Проверка разбора текста для голоса в silero_server.py — те же случаи, что
в src/speech.rs. Запуск: python src-tauri/assets/test_speech.py"""

import os

HERE = os.path.dirname(os.path.abspath(__file__))
source = open(os.path.join(HERE, "silero_server.py"), encoding="utf-8").read()
names = {"__file__": os.path.join(HERE, "silero_server.py")}
exec("import os, json, re\n" + source[source.index("HERE = os.path"):source.index("def chunks(")], names)
speakable = names["speakable"]

CASES = [
    ("Порт 443 открыт.", "Порт четыреста сорок три открыт."),
    ("Нагрузка 75%.", "Нагрузка семьдесят пять процентов."),
    ("Осталось 2 ГБ, а нужно 1 ГБ", "Осталось два гигабайта, а нужно один гигабайт"),
    ("адрес 203.0.113.5", "адрес двести три точка ноль точка сто тринадцать точка пять"),
    ("было -5 градусов", "было минус пять градусов"),
    ("стоит $20", "стоит двадцать долларов"),
    ("kubectl get pods", "кубкатл гет поды"),
    ("Загрузка CPU выросла", "Загрузка си-пи-ю выросла"),
    ("протоколы TCP/UDP", "протоколы ти-си-пи или ю-ди-пи"),
    ("конфиг /etc/nginx/nginx.conf", "конфиг этк энджинкс энджинкс конф"),
    ("Под в k8s.", "Под в кубернетис."),
    ("Флаг OOMKilled", "Флаг оу-оу-эм килд"),
    ("команда ls", "команда эл-эс"),
    ("RBAC", "ар-би-эй-си"),
    ("grafana", "графана"),
    ("в 10:30 запуск", "в десять тридцать запуск"),
    ("ответ за 120 ms", "ответ за сто двадцать миллисекунд"),
    ("Kubernetes 1.30.2", "кубернетис один точка тридцать точка два"),
    ("Контейнер — обычный процесс на общем ядре.", "Контейнер — обычный процесс на общем ядре."),
]

failed = 0
for text, want in CASES:
    got = speakable(text)
    if got != want:
        failed += 1
        print(f"НЕ ТАК: {text!r}\n  ждали:   {want!r}\n  вышло:   {got!r}")
print(f"{len(CASES) - failed} из {len(CASES)}")
raise SystemExit(1 if failed else 0)
