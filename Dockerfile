# Постоянная демонстрация интерфейса: та же страница, что открывается локально
# командой `npm run dev`, но живёт независимо от терминала.
#
# Отдаёт только каталог src/ — это статика без сборки: ни бандлера, ни зависимостей,
# поэтому образ получается на одном node и без npm install.
#
#   docker build -t sufler-demo .
#   docker run -d --name sufler-demo --restart unless-stopped -p 5173:5173 sufler-demo
#
# Важно: это демонстрация попапа внутри одной веб-страницы. Само приложение работает
# поверх других программ и ставится установщиком из раздела Releases — в браузере
# доступа к чужим окнам нет ни у кого.

FROM node:20-alpine

WORKDIR /app

COPY src ./src
COPY scripts/dev-server.mjs ./scripts/dev-server.mjs

ENV PORT=5173
EXPOSE 5173

# Запуск не от root: сервер отдаёт файлы и больше ничего не делает,
# лишние права ему ни к чему.
USER node

CMD ["node", "scripts/dev-server.mjs"]
