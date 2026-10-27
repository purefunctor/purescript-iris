import * as $foreign from "./foreign.js";
export function countdown(start) {
  return () => {
    const counter = { value: start };
    const $value = [];
    const seen = { value: $value };
    const $function = (section77) => section77 > (0 | 0);
    while (true) {
      let $condition;
      $condition = $function(counter.value);
      if (!$condition) {
        break;
      }
      const current = counter.value;
      seen.value = [...seen.value, current];
      const $value$1 = current - (1 | 0) | 0;
      counter.value = $value$1;
    }
    return seen.value;
  };
}
export function countUntil(limit) {
  return () => {
    const counter = { value: 0 | 0 };
    while (true) {
      let $condition;
      const count = counter.value = counter.value + (1 | 0) | 0;
      const $value = count >= limit;
      $condition = $value;
      if ($condition) {
        break;
      }
    }
    return counter.value;
  };
}
export function doubleUntil(limit) {
  const value = { value: 1 | 0 };
  const $function = (section233) => section233 < limit;
  while (true) {
    let $condition;
    $condition = $function(value.value);
    if (!$condition) {
      break;
    }
    value.value = value.value * (2 | 0) | 0;
  }
  return value.value;
}
export const hasNext = $foreign["hasNext"];
export const next = $foreign["next"];
export const drain = /* @__PURE__ */ (() => {
  return () => {
    while (hasNext()) {
      next();
    }
  };
})();
