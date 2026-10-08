// Телефон: экраны Ноа (обучение, практика) — окна компьютера, им нужны
// правила для узкого экрана (phone.css): без кнопок окна, во всю ширину,
// темы — выдвижным списком. Панели вкладок нет: на iPhone программа — только
// модуль обучения.

export function mountPhoneNav() {
  if (document.querySelector("link[data-phone-css]")) return;
  const link = document.createElement("link");
  link.rel = "stylesheet";
  link.href = new URL("../styles/phone.css", import.meta.url).href;
  link.dataset.phoneCss = "";
  document.head.append(link);
}
