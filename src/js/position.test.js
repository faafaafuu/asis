// Тесты позиционирования попапа. Запуск: `npm test`.
import { test } from "node:test";
import assert from "node:assert/strict";

import { placePopup, popupWidth, isAnchorVisible } from "./position.js";

const VIEWPORT = { width: 1280, height: 800 };
const SIZE = { width: 400, height: 200 };

const anchor = (top, extra = {}) => ({
  left: 500,
  right: 600,
  top,
  bottom: top + 22,
  width: 100,
  ...extra,
});

test("окно встаёт над выделением и центрируется по нему", () => {
  const { left, top, flipped } = placePopup({ anchor: anchor(400), size: SIZE, viewport: VIEWPORT });
  assert.equal(left, 350, "центр окна совпадает с центром выделения");
  assert.equal(top, 188, "зазор 12px над выделением");
  assert.equal(flipped, false);
});

test("если сверху не помещается — зеркалится под выделение", () => {
  const { top, flipped } = placePopup({ anchor: anchor(40), size: SIZE, viewport: VIEWPORT });
  assert.equal(top, 74, "bottom + зазор");
  assert.equal(flipped, true);
});

test("окно не вылезает за края по горизонтали", () => {
  const left = placePopup({
    anchor: anchor(400, { left: 0, right: 40, width: 40 }),
    size: SIZE,
    viewport: VIEWPORT,
  }).left;
  assert.equal(left, 12, "прижато к левому краю минус inset");

  const right = placePopup({
    anchor: anchor(400, { left: 1270, right: 1280, width: 10 }),
    size: SIZE,
    viewport: VIEWPORT,
  }).left;
  assert.equal(right, 868, "прижато к правому краю минус inset");
});

test("выделение уехало выше экрана — окно всё равно видно", () => {
  // Так бывает на телефоне: страницу прокрутили, а попап открыли кнопкой.
  const { top } = placePopup({ anchor: anchor(-420), size: SIZE, viewport: { width: 390, height: 700 } });
  assert.ok(top >= 12, `окно должно остаться на экране, получено top=${top}`);
});

test("выделение уехало ниже экрана — окно всё равно видно", () => {
  const { top } = placePopup({ anchor: anchor(2000), size: SIZE, viewport: VIEWPORT });
  assert.ok(top + SIZE.height <= VIEWPORT.height - 12 + 1, `получено top=${top}`);
});

test("на узком экране ширина ужимается до 100vw − 24px", () => {
  assert.equal(popupWidth({ expanded: false, viewportWidth: 1280 }), 400);
  assert.equal(popupWidth({ expanded: true, viewportWidth: 1280 }), 480);
  assert.equal(popupWidth({ expanded: false, viewportWidth: 390 }), 366);
});

test("выход выделения за пределы вьюпорта определяется с запасом", () => {
  assert.equal(isAnchorVisible(anchor(400), VIEWPORT), true);
  assert.equal(isAnchorVisible(anchor(-30), VIEWPORT), true, "запас 40px ещё не исчерпан");
  assert.equal(isAnchorVisible(anchor(-200), VIEWPORT), false);
  assert.equal(isAnchorVisible(anchor(1000), VIEWPORT), false);
});
