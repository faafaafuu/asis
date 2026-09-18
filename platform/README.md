# Площадка NOAH

Сайт с библиотекой модулей, кабинетом автора и MCP по ссылке. Для работы нужен только Node 24. Сторонних пакетов нет.

```
platform/
  server/   сервер: node:http + node:sqlite
  web/      сайт: статические файлы без сборки
  deploy/   юнит systemd
```

## Запуск

```bash
node platform/server/server.mjs
```

| Переменная | Что задаёт | По умолчанию |
|---|---|---|
| `NOAH_PORT`, `NOAH_HOST` | адрес сервера | `8795`, `0.0.0.0` |
| `NOAH_DATA` | папка с базой `noah.db` | `platform/data` |
| `NOAH_SEED` | модули, которые кладутся в пустую базу | `modules/index.json` |
| `NOAH_SECURE` | `1` — куки только по HTTPS (включать вместе с доменом) | выключено |
| `NOAH_PUBLIC_URL` | внешний адрес сайта; из него строятся обратные ссылки входа | `http://84.247.166.53:8795` |

На сервере юнит `deploy/noah-platform.service` читает дополнительные переменные из `/etc/noah-platform.env` с правами `600`. В этот файл кладутся ключи входа.

## Вход через другие сервисы

Кнопка сервиса появляется на странице входа, как только у сервиса заданы оба ключа. После правки `/etc/noah-platform.env` нужен перезапуск: `systemctl restart noah-platform`.

В кабинете, в блоке «Способы входа», к одному аккаунту можно привязать несколько сервисов. Там же их можно отвязать. Единственный способ входа отвязать нельзя, пока не задан пароль.

Если сервис прислал подтверждённую почту, которая уже есть в базе, человек попадает в существующий аккаунт. Почту от VK подтверждённой не считаем: по ней заводится отдельный аккаунт.

| Сервис | Переменные | Где получить ключи |
|---|---|---|
| Google | `NOAH_GOOGLE_ID`, `NOAH_GOOGLE_SECRET` | Google Cloud Console → APIs & Services → Credentials → OAuth client ID (Web application) |
| GitHub | `NOAH_GITHUB_ID`, `NOAH_GITHUB_SECRET` | Settings → Developer settings → OAuth Apps |
| X | `NOAH_X_ID`, `NOAH_X_SECRET` | developer.x.com → проект → User authentication settings (OAuth 2.0, Web App) |
| Яндекс | `NOAH_YANDEX_ID`, `NOAH_YANDEX_SECRET` | oauth.yandex.ru → создать приложение, доступы «логин» и «почта» |
| VK | `NOAH_VK_ID`, `NOAH_VK_SECRET` | id.vk.com → приложение → доверенный redirect URL |
| Telegram | `NOAH_TG_BOT_TOKEN`, `NOAH_TG_BOT_NAME` | @BotFather → новый бот только для входа |

Адрес возврата, который нужно указать у сервиса:

```
${NOAH_PUBLIC_URL}/auth/<сервис>/callback
```

Например, `https://noah.example/auth/google/callback`. Google и большинство других сервисов принимают адрес возврата только с доменом и HTTPS. Поэтому сначала подключается домен, затем задаётся `NOAH_PUBLIC_URL`, и только потом выпускаются ключи.

### Telegram

Для входа через Telegram домен не нужен. Сайт выдаёт ссылку `t.me/<бот>?start=<код>`, человек нажимает «Старт», а сервер забирает подтверждение из `getUpdates` и впускает человека. Коду даётся 5 минут.

Бот должен быть отдельным: сервер сам читает его обновления. Если тот же бот отвечает в Telegram от имени Ноа или у него включён webhook, вход работать не будет.

Пример `/etc/noah-platform.env`:

```
NOAH_PUBLIC_URL=https://noah.example
NOAH_GOOGLE_ID=…
NOAH_GOOGLE_SECRET=…
NOAH_TG_BOT_TOKEN=…
NOAH_TG_BOT_NAME=noah_login_bot
```

## Проверка

```bash
node platform/server/selftest.mjs
```
