import * as $foreign from "./foreign.js";
export function countdown(start) {
  return () => {
    let counter = start;
    const $value = [];
    let seen = $value;
    const $function = (section77) => section77 > (0 | 0);
    while (true) {
      let $condition;
      $condition = $function(counter);
      if (!$condition) {
        break;
      }
      const current = counter;
      seen = [...seen, current];
      const $value$1 = current - (1 | 0) | 0;
      counter = $value$1;
    }
    return seen;
  };
}
export function countUntil(limit) {
  return () => {
    let counter = 0 | 0;
    while (true) {
      let $condition;
      const count = counter = counter + (1 | 0) | 0;
      const $value = count >= limit;
      $condition = $value;
      if ($condition) {
        break;
      }
    }
    return counter;
  };
}
export function doubleUntil(limit) {
  let value = 1 | 0;
  const $function = (section233) => section233 < limit;
  while (true) {
    let $condition;
    $condition = $function(value);
    if (!$condition) {
      break;
    }
    value = value * (2 | 0) | 0;
  }
  return value;
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
