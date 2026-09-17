// Стандарт модуля NOAH — те же правила, что проверяет приложение
// (src-tauri/src/module_kit.rs). Сайт принимает к публикации только модуль,
// который прошёл их здесь и живую проверку у автора в Ноа.

export const FORMAT = 1;

const RUNTIMES = ["node", "npx", "python", "python3", "py", "uv", "uvx", "deno", "bun"];
const FILE_TYPES = ["js", "mjs", "cjs", "ts", "py", "json", "txt", "md", "csv", "yaml", "yml", "toml", "html", "css"];
const SERVICE_FILES = ["checked.json", "secrets.json", "status.json", "server.log", ".updating", "module.json"];
const MAX_FILES = 40;
const MAX_FILE_BYTES = 1024 * 1024;
const MAX_TOTAL_BYTES = 5 * 1024 * 1024;

export const CATEGORIES = ["work", "home", "finance", "dev", "health", "media", "other"];

const len = (text) => [...String(text ?? "").trim()].length;

export const validId = (id) => typeof id === "string" && /^[a-z0-9][a-z0-9-]{0,39}$/.test(id);
const validSecretName = (name) => typeof name === "string" && /^[A-Z][A-Z0-9_]{0,47}$/.test(name);
const shellMeta = (text) => /[&|<>^`"\r\n]/.test(String(text));
const looksSecret = (key) => /KEY|TOKEN|SECRET|PASSWORD|PASS|AUTH|COOKIE/i.test(key);

export function validFileName(name) {
  const dot = name.lastIndexOf(".");
  if (dot <= 0) return false;
  return (
    name.length <= 80 &&
    !name.startsWith(".") &&
    !SERVICE_FILES.includes(name) &&
    /^[A-Za-z0-9._-]+$/.test(name) &&
    FILE_TYPES.includes(name.slice(dot + 1).toLowerCase())
  );
}

function isPackage(manifest) {
  const mcp = manifest.mcp ?? {};
  return !String(mcp.command ?? "").includes("%MODULE_DIR%") && !(mcp.args ?? []).some((a) => String(a).includes("%MODULE_DIR%"));
}

/** Нарушения стандарта. Пустой список — модуль годится. */
export function lint(manifest, files) {
  const problems = [];
  const need = (ok, text) => {
    if (!ok) problems.push(text);
  };
  if (!manifest || typeof manifest !== "object") return ["module.json — объект."];
  const mcp = manifest.mcp ?? {};
  const args = Array.isArray(mcp.args) ? mcp.args : [];
  const env = mcp.env && typeof mcp.env === "object" ? mcp.env : {};
  const command = String(mcp.command ?? "").trim();

  need(validId(manifest.id), "id — латиница в нижнем регистре, цифры и дефис, до 40 знаков.");
  need(len(manifest.title) >= 1 && len(manifest.title) <= 40, "title — 1–40 знаков.");
  need(len(manifest.icon) >= 1 && len(manifest.icon) <= 2, "icon — один символ.");
  need(len(manifest.about) >= 5 && len(manifest.about) <= 140, "about — 5–140 знаков.");
  need(len(manifest.voice) >= 5, "voice — пример фразы.");
  need(command.length > 0, "mcp.command — чем запускать сервер.");
  need(!command || command.startsWith("%MODULE_DIR%") || RUNTIMES.includes(command), `mcp.command «${command}» не разрешён.`);
  need(!shellMeta(command) && args.every((a) => !shellMeta(a)), "В mcp.command и mcp.args нельзя & | < > ^ ` и кавычки.");
  need(args.length <= 20, "mcp.args — не больше 20.");
  for (const [key, value] of Object.entries(env)) {
    need(validSecretName(key), `mcp.env: имя «${key}» — заглавная латиница, цифры, подчёркивание.`);
    need(!looksSecret(key) || !String(value).trim(), `mcp.env.${key} похоже на ключ — объявите его в secrets.`);
  }
  const names = new Set();
  for (const secret of manifest.secrets ?? []) {
    need(validSecretName(secret?.name), `secrets: имя «${secret?.name}» не подходит.`);
    need(len(secret?.title) > 0, `secrets.${secret?.name}: нужен title.`);
    need(len(secret?.hint) > 0, `secrets.${secret?.name}: нужен hint.`);
    need(!names.has(secret?.name), `secrets: «${secret?.name}» повторяется.`);
    names.add(secret?.name);
  }
  if (!isPackage(manifest)) need((manifest.tests ?? []).length > 0, "tests — нужен хотя бы один тест.");

  const entries = Object.entries(files ?? {});
  need(entries.length <= MAX_FILES, `Файлов больше ${MAX_FILES}.`);
  let total = 0;
  for (const [name, text] of entries) {
    const bytes = Buffer.byteLength(String(text));
    total += bytes;
    need(validFileName(name), `Файл «${name}» не подходит по имени или типу.`);
    need(typeof text === "string" && bytes > 0, `Файл «${name}» пустой или не текст.`);
    need(bytes <= MAX_FILE_BYTES, `Файл «${name}» больше 1 МБ.`);
  }
  need(total <= MAX_TOTAL_BYTES, "Файлы вместе больше 5 МБ.");
  for (const arg of [command, ...args]) {
    const text = String(arg);
    if (text.startsWith("%MODULE_DIR%")) {
      const name = text.slice("%MODULE_DIR%".length).replace(/^[\\/]+/, "");
      if (name) need(Object.hasOwn(files ?? {}, name), `mcp ссылается на «${name}», а такого файла нет.`);
    }
  }
  return problems;
}
