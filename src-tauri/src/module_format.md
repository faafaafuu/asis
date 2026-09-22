# Регламент модулей Ноа (v1)

Модуль — это MCP-сервер и его описание `module.json`. Ноа ставит модуль только
после проверки: правила ниже, живой запуск, тесты, устойчивость к ошибкам. Модуль,
не прошедший проверку, не запускается, а ты получаешь отчёт с тем, что исправить.

## Порядок работы

1. Прочитай этот регламент целиком.
2. Вызови `environment`: узнай, что установлено (Node.js, Python). Пиши сервер под
   то, что есть. Нет ничего — попроси пользователя поставить Node.js LTS.
3. Уточни у пользователя: что модуль делает, какими фразами его будут звать, нужны
   ли ключи или аккаунты и где их взять.
4. Напиши `module.json` и файлы сервера, отправь `create_module`.
5. Отчёт с ошибками — исправь всё перечисленное и отправь снова. Повторяй, пока
   проверка не пройдёт. До успешного отчёта не говори, что модуль готов.
6. Нужны ключи — объяви их в `secrets` и попроси пользователя ввести их в окне Ноа
   (Модули → модуль → «Ключи»). Значения ключей не спрашивай и не пиши в чат. Когда
   пользователь ввёл их — `check_module`.
7. После успеха скажи пользователю фразу из `voice`.

Менять модуль — тем же `create_module` с тем же `id`: работающая версия не
трогается, пока новая не прошла проверку.

## module.json

```json
{
  "id": "weather",
  "title": "Погода",
  "icon": "☀",
  "about": "Погода сейчас в любом городе",
  "voice": "«Ноа, какая погода в Казани»",
  "version": "1.0.0",
  "mcp": {
    "command": "node",
    "args": ["%MODULE_DIR%\\server.mjs"],
    "env": {}
  },
  "secrets": [],
  "tests": [
    { "tool": "weather", "args": { "city": "Казань" }, "expect": "Казань", "about": "погода в Казани" }
  ],
  "untested": {}
}
```

| Поле | Правило |
|---|---|
| `id` | латиница в нижнем регистре, цифры, дефис; до 40 знаков; имя папки модуля |
| `title` | название на плитке, 1–40 знаков |
| `icon` | один символ или эмодзи |
| `about` | что умеет, одной фразой, 5–140 знаков |
| `voice` | пример фразы пользователя |
| `mcp.command` | `node`, `npx`, `python`, `python3`, `py`, `uv`, `uvx`, `deno`, `bun` — или файл модуля через `%MODULE_DIR%` |
| `mcp.args` | аргументы, до 20; без `& \| < > ^ ` и кавычек; `%MODULE_DIR%` — папка модуля |
| `mcp.env` | переменные окружения; имена — `ЗАГЛАВНЫЕ_С_ПОДЧЁРКИВАНИЕМ`; ключей здесь быть не может |
| `secrets` | ключи от пользователя: `name` (имя переменной), `title`, `hint` (где взять), `optional` |
| `tests` | проверки: `tool`, `args`, `expect` (подстрока ответа, без учёта регистра), `about` |
| `untested` | инструменты, которые нельзя вызывать в проверке (отправляют, платят, удаляют): `{ "имя": "причина" }` |
| `window` | окно модуля, если оно нужно: `file` (файл разметки), `title`, `width`, `height` |

Ключи приходят серверу переменными окружения с именами из `secrets`. В аргументах
их можно подставить как `%ИМЯ%`, но лучше читать из окружения.

## Окно модуля

Модулю окно нужно не всегда: голоса обычно хватает. Но если данные удобнее
видеть — курс, список, форму, — модуль описывает окно **одной страницей
разметки**, а рисует его Ноа: своей рамкой, своей темой и своими шрифтами.
Окно модуля выглядит как окно Ноа.

Чего в окне быть не может:

- **своего сервера и адресов.** Ни `http://`, ни `https://`, ни `localhost`:
  открывать вместо окна вкладку браузера — значит отдать человека браузеру;
- **своих скриптов.** `<script>` и `<iframe>` запрещены: чужой код в окне
  помощника не исполняется;
- **своего оформления.** `<style>` и `style="…"` запрещены: цвета, шрифты и
  отступы даёт тема Ноа (тёмная, светлая, неон, синтвейв — какую выбрал
  человек). Модуль, раскрашенный по-своему, выпадает из программы.

Что модуль пишет в разметке: заголовки `h2`, `h3`, абзацы, списки, поля
`input`, `select`, `textarea` в `label`, кнопки. Ноа сама оформит их.

Кнопка связывается с инструментом модуля атрибутами:

| Атрибут | Где | Что значит |
|---|---|---|
| `data-call="имя"` | на кнопке | какой инструмент модуля вызвать |
| `data-field="имя"` | на поле | значение поля уйдёт аргументом с этим именем |
| `data-arg-имя="значение"` | на кнопке | постоянный аргумент |
| `data-out="имя"` | на любом элементе | куда положить ответ; по умолчанию `result` |

```json
"window": { "file": "panel.html", "title": "Погода", "width": 420, "height": 380 }
```

```html
<h2>Погода сейчас</h2>
<div class="row">
  <label>Город <input data-field="city" value="Казань" /></label>
  <button data-call="weather">Узнать</button>
</div>
<p data-out="result"></p>
```

Ответ инструмента Ноа положит в элемент с `data-out`. Ничего больше от модуля
не нужно: ни кода, ни стилей, ни сервера.

Рамку окна делает Ноа, и она одинаковая у всех модулей: заголовок со значком,
за который окно **двигают**, кнопки «свернуть» и «закрыть», Esc — закрыть. Своего
заголовка, своих кнопок закрытия и своей полосы для перетаскивания модуль не
рисует: два заголовка в одном окне — это окно, которое нельзя сдвинуть за
верхний край, потому что верхний край принадлежит не той рамке.

Настройки модуля — тоже в его окне, а не в настройках программы. Ключи из
`secrets` Ноа показывает внизу окна сама: человек открыл модуль — и всё, что к
модулю относится, у него перед глазами. Просить его идти за настройкой в другое
окно значит заставлять искать.

## Файлы

- Только в корне папки модуля, без подпапок; имена — латиница, цифры, `.`, `-`, `_`.
- Типы: js, mjs, cjs, ts, py, json, txt, md, csv, yaml, yml, toml, html, css.
  Файл окна — html; отдельный css модулю не нужен, оформление даёт Ноа.
  Исполняемых файлов (exe, bat, cmd, ps1, dll) быть не может.
- До 40 файлов, каждый до 1 МБ, вместе до 5 МБ.
- Только стандартная библиотека: `npm install` и `pip install` не выполняются.
  Нужен пакет — используй готовый MCP-сервер из npm через `npx -y пакет`.
- Файлы и данные модуля — только в его папке (`NOA_MODULE_DIR` в окружении).

## Сервер

- MCP (JSON-RPC 2.0) через stdin/stdout, одно сообщение — одна строка.
- Отвечает на `initialize`, `tools/list`, `tools/call`; на прочие запросы с `id` —
  ошибкой `-32601`; уведомления (без `id`) — молча пропускает.
- В stdout — только ответы протокола. Любая отладка — в stderr (попадает в журнал).
- Не падает: любое исключение в инструменте ловится и возвращается как
  `{ "content": [{ "type": "text", "text": "что не так" }], "isError": true }`.
- Неизвестный инструмент и неверные аргументы — ответ `isError`, не молчание.
- Сеть — с тайм-аутом до 15 секунд; ответ инструмента — до 8 секунд.
- Ответ — короткий текст по-русски, до 4000 знаков: Ноа читает его вслух.
- Сервер живёт всё время работы Ноа: не держи лишнего в памяти, не пиши без
  конца на диск.

## Инструменты

- До 25 в модуле. Имя — латиница, цифры, `_`, `-`.
- `description` — по-русски, от 15 знаков: по нему Ноа решает, какую фразу превратить
  в вызов. Пиши, что делает инструмент и когда его звать.
- `inputSchema` — `{"type": "object", "properties": {…}, "required": […]}`; у каждого
  поля `type` и `description`; все `required` описаны в `properties`.
- У каждого инструмента — тест в `tests` или причина в `untested`.

## Что проверяет Ноа

1. Описание и файлы — по правилам выше.
2. Ключи из `secrets` введены пользователем.
3. Сервер запускается и отвечает на `initialize` (30 с; для `npx` — 180 с).
4. `tools/list`: имена, описания, схемы.
5. Каждый тест: ответ без ошибки, не пустой, до 4000 знаков, содержит `expect`.
6. Вызов несуществующего инструмента и вызов с пустыми аргументами — ответ, а не
   зависание; сервер после них жив.
7. В stdout нет посторонних строк.

Прошедшая версия запоминается по отпечатку файлов; изменённый без проверки модуль
Ноа не запускает. Упавший сервер Ноа перезапускает; после трёх падений за десять
минут модуль останавливается до исправления.

## Пример: server.mjs (Node.js)

```js
import readline from "node:readline";

const tools = [{
  name: "weather",
  description: "Погода сейчас в указанном городе: температура и ветер",
  inputSchema: {
    type: "object",
    properties: { city: { type: "string", description: "Город, например «Казань»" } },
    required: ["city"],
  },
}];

async function getJson(url) {
  const response = await fetch(url, { signal: AbortSignal.timeout(15000) });
  if (!response.ok) throw new Error(`сервис ответил ${response.status}`);
  return response.json();
}

async function weather({ city }) {
  if (!city) throw new Error("не назван город");
  const geo = await getJson(`https://geocoding-api.open-meteo.com/v1/search?name=${encodeURIComponent(city)}&count=1&language=ru`);
  const place = geo.results?.[0];
  if (!place) return `Не нашёл город «${city}».`;
  const now = await getJson(`https://api.open-meteo.com/v1/forecast?latitude=${place.latitude}&longitude=${place.longitude}&current=temperature_2m,wind_speed_10m`);
  return `${place.name}: ${Math.round(now.current.temperature_2m)}°, ветер ${Math.round(now.current.wind_speed_10m)} км/ч.`;
}

const handlers = { weather };
const send = (message) => process.stdout.write(JSON.stringify(message) + "\n");

readline.createInterface({ input: process.stdin }).on("line", async (line) => {
  let request;
  try { request = JSON.parse(line); } catch { return; }
  if (request.id === undefined) return;
  const reply = (result) => send({ jsonrpc: "2.0", id: request.id, result });
  const text = (value, isError = false) => reply({ content: [{ type: "text", text: String(value) }], isError });

  if (request.method === "initialize") {
    return reply({
      protocolVersion: request.params?.protocolVersion ?? "2024-11-05",
      capabilities: { tools: {} },
      serverInfo: { name: "weather", version: "1.0.0" },
    });
  }
  if (request.method === "tools/list") return reply({ tools });
  if (request.method === "tools/call") {
    const handler = handlers[request.params?.name];
    if (!handler) return text(`Нет инструмента «${request.params?.name}».`, true);
    try {
      return text(await handler(request.params?.arguments ?? {}));
    } catch (err) {
      console.error(err);
      return text(`Не получилось: ${err.message}`, true);
    }
  }
  send({ jsonrpc: "2.0", id: request.id, error: { code: -32601, message: "нет такого метода" } });
});
```

## Пример: server.py (Python)

```python
import json, sys, urllib.parse, urllib.request

TOOLS = [{
    "name": "weather",
    "description": "Погода сейчас в указанном городе: температура и ветер",
    "inputSchema": {
        "type": "object",
        "properties": {"city": {"type": "string", "description": "Город, например «Казань»"}},
        "required": ["city"],
    },
}]

def get_json(url):
    with urllib.request.urlopen(url, timeout=15) as response:
        return json.load(response)

def weather(args):
    city = args.get("city")
    if not city:
        raise ValueError("не назван город")
    geo = get_json("https://geocoding-api.open-meteo.com/v1/search?count=1&language=ru&name=" + urllib.parse.quote(city))
    places = geo.get("results") or []
    if not places:
        return f"Не нашёл город «{city}»."
    p = places[0]
    now = get_json(f"https://api.open-meteo.com/v1/forecast?latitude={p['latitude']}&longitude={p['longitude']}&current=temperature_2m,wind_speed_10m")["current"]
    return f"{p['name']}: {round(now['temperature_2m'])}°, ветер {round(now['wind_speed_10m'])} км/ч."

HANDLERS = {"weather": weather}

def send(message):
    sys.stdout.write(json.dumps(message, ensure_ascii=False) + "\n")
    sys.stdout.flush()

for line in sys.stdin:
    try:
        request = json.loads(line)
    except ValueError:
        continue
    if "id" not in request:
        continue
    rid, method, params = request["id"], request.get("method"), request.get("params") or {}
    if method == "initialize":
        send({"jsonrpc": "2.0", "id": rid, "result": {
            "protocolVersion": params.get("protocolVersion", "2024-11-05"),
            "capabilities": {"tools": {}},
            "serverInfo": {"name": "weather", "version": "1.0.0"},
        }})
    elif method == "tools/list":
        send({"jsonrpc": "2.0", "id": rid, "result": {"tools": TOOLS}})
    elif method == "tools/call":
        handler = HANDLERS.get(params.get("name"))
        try:
            if handler is None:
                raise ValueError(f"нет инструмента «{params.get('name')}»")
            text, is_error = handler(params.get("arguments") or {}), False
        except Exception as err:
            print(repr(err), file=sys.stderr)
            text, is_error = f"Не получилось: {err}", True
        send({"jsonrpc": "2.0", "id": rid, "result": {"content": [{"type": "text", "text": text}], "isError": is_error}})
    else:
        send({"jsonrpc": "2.0", "id": rid, "error": {"code": -32601, "message": "нет такого метода"}})
```

Для Python: `"command": "python"`, `"args": ["%MODULE_DIR%\\server.py"]` (или `py`,
если `environment` показал только его).

## Отправка

`create_module` с `module` (как выше) и `files`: `{ "server.mjs": "<текст сервера>" }`.
