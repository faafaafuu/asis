Модуль Ноа — папка `%APPDATA%\app.sufler.popup\modules\<id>\` с описанием `module.json` и файлами MCP-сервера.

## module.json

```json
{
  "id": "weather",
  "title": "Погода",
  "icon": "☀",
  "about": "Погода в любом городе",
  "voice": "«Ноа, какая погода в Казани»",
  "mcp": {
    "command": "node",
    "args": ["%MODULE_DIR%\\server.mjs"],
    "env": {}
  }
}
```

- `id` — латиница в нижнем регистре, цифры, дефис; до 40 знаков.
- `title`, `icon`, `about`, `voice` — название, один символ, что умеет, пример фразы.
- `mcp.command` — чем запускать: `node`, `python`, `npx`, `uvx` или путь к программе.
- `mcp.args` — аргументы. `%MODULE_DIR%` — папка модуля, `%ИМЯ%` — переменная окружения.
- `mcp.env` — переменные окружения сервера, например ключ API.

## Сервер

- Говорит MCP (JSON-RPC 2.0) через стандартные ввод и вывод, по сообщению в строке:
  отвечает на `initialize`, `tools/list` и `tools/call`. В stdout — только ответы
  протокола; отладка — в stderr, она попадает в `server.log` в папке модуля.
- Инструменты описаны по-русски и понятно: по описанию Ноа решает, какую фразу
  пользователя превратить в вызов. Ответ инструмента — короткий текст, Ноа
  перескажет его голосом.
- Без лишних зависимостей, только стандартная библиотека. Узнай у пользователя, что
  у него установлено (Node.js или Python), и пиши под это.

## Пример: server.mjs (Node.js, без зависимостей)

```js
import readline from "node:readline";

const tools = [{
  name: "weather",
  description: "Погода сейчас в городе",
  inputSchema: { type: "object", properties: { city: { type: "string" } }, required: ["city"] },
}];

async function weather({ city }) {
  const geo = await (await fetch(`https://geocoding-api.open-meteo.com/v1/search?name=${encodeURIComponent(city)}&count=1&language=ru`)).json();
  const place = geo.results?.[0];
  if (!place) return `Не нашёл город «${city}».`;
  const now = await (await fetch(`https://api.open-meteo.com/v1/forecast?latitude=${place.latitude}&longitude=${place.longitude}&current=temperature_2m,wind_speed_10m`)).json();
  return `${place.name}: ${Math.round(now.current.temperature_2m)}°, ветер ${Math.round(now.current.wind_speed_10m)} км/ч.`;
}

const send = (message) => process.stdout.write(JSON.stringify(message) + "\n");

readline.createInterface({ input: process.stdin }).on("line", async (line) => {
  const request = JSON.parse(line);
  if (request.id === undefined) return;
  const reply = (result) => send({ jsonrpc: "2.0", id: request.id, result });
  if (request.method === "initialize") {
    return reply({ protocolVersion: request.params.protocolVersion, capabilities: { tools: {} }, serverInfo: { name: "weather", version: "1.0.0" } });
  }
  if (request.method === "tools/list") return reply({ tools });
  if (request.method === "tools/call") {
    try {
      return reply({ content: [{ type: "text", text: await weather(request.params.arguments) }] });
    } catch (err) {
      return reply({ content: [{ type: "text", text: String(err) }], isError: true });
    }
  }
  send({ jsonrpc: "2.0", id: request.id, error: { code: -32601, message: "нет такого метода" } });
});
```

## Как поставить

`create_module` с `module` (как выше) и `files`: `{ "server.mjs": "<текст сервера>" }`.
Через 5–10 секунд проверь `list_modules`: если сервер упал, его ошибка будет в журнале.
