// Статический сервер для разработки фронтенда попапа в обычном браузере.
// Нужен для двух вещей:
//   1. `npm run dev` + открыть http://localhost:5173/demo.html — демо-документ из
//      дизайн-референса: там попап работает целиком в вебе (Selection API), что позволяет
//      проверять вёрстку/анимации/позиционирование без сборки Tauri.
//   2. `tauri dev` берёт этот же адрес как devUrl.
import { createServer } from "node:http";
import { readFile, stat } from "node:fs/promises";
import { extname, join, normalize, sep } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = fileURLToPath(new URL("../src/", import.meta.url));
const PORT = Number(process.env.PORT ?? 5173);

const MIME = {
  ".html": "text/html; charset=utf-8",
  ".js": "text/javascript; charset=utf-8",
  ".css": "text/css; charset=utf-8",
  ".json": "application/json; charset=utf-8",
  ".woff2": "font/woff2",
  ".png": "image/png",
  ".svg": "image/svg+xml",
};

const server = createServer(async (req, res) => {
  const url = new URL(req.url, "http://localhost");
  let rel = decodeURIComponent(url.pathname);
  if (rel === "/") rel = "/demo.html";

  // Не выпускаем запрос за пределы src/ (простейшая защита от ../).
  const path = normalize(join(ROOT, rel));
  if (!path.startsWith(ROOT.replace(new RegExp(`${sep}$`), "") + sep)) {
    res.writeHead(403).end("403");
    return;
  }

  try {
    const info = await stat(path);
    if (info.isDirectory()) throw new Error("directory");
    const body = await readFile(path);
    res.writeHead(200, {
      "content-type": MIME[extname(path)] ?? "application/octet-stream",
      "cache-control": "no-store",
    });
    res.end(body);
  } catch {
    res.writeHead(404, { "content-type": "text/plain; charset=utf-8" });
    res.end("404 — нет такого файла в src/");
  }
});

server.listen(PORT, () => {
  console.log(`Суфлёр · dev-сервер: http://localhost:${PORT}/demo.html`);
});
