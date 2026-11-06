// Вход в «NOAH Учёба» в браузере по короткому адресу /learn — как go.js для
// Ноа онлайн: CDN зеркала держит голый адрес в памяти, поэтому здесь только
// пересылка на адрес с одноразовой меткой.
// Остальные параметры (ошибка входа) — дальше, в приложение.
const params = new URLSearchParams(location.search);
params.set("v", Date.now().toString(36));
location.replace(`/app/learn.html?${params}${location.hash}`);
