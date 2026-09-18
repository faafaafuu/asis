// Адреса, общие для страниц площадки.

export const RELEASES = "/download";
// Адрес сайта для ссылок, которые уходят наружу: открыли по IP — всё равно домен.
export const SITE = /^[\d.]+$/.test(location.hostname) ? "https://noahlab.ru" : location.origin;
export const REPO = "https://github.com/faafaafuu/asis";
export const STANDARD_DOC = "#/docs/module-standard";
