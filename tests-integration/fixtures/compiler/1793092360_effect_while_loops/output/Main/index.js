import * as Control_Monad_ST_Internal from "../Control.Monad.ST.Internal/index.js";
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
      let $unit;
      const section105 = seen;
      const $value$1 = [...section105, current];
      seen = $value$1;
      $unit = undefined;
      const $value$2 = current - (1 | 0) | 0;
      counter = $value$2;
    }
    return seen;
  };
}
export function countUntil(limit) {
  return () => {
    let counter = 0 | 0;
    while (true) {
      let $condition;
      let count;
      const section165 = counter;
      const $value = section165 + (1 | 0) | 0;
      count = counter = $value;
      $condition = count >= limit;
      if ($condition) {
        break;
      }
    }
    return counter;
  };
}
export function doubleUntil(limit) {
  const $function = Control_Monad_ST_Internal.run;
  const $effect = () => {
    let value = 1 | 0;
    const $function$1 = (section233) => section233 < limit;
    while (true) {
      let $condition;
      $condition = $function$1(value);
      if (!$condition) {
        break;
      }
      const section252 = value;
      const $value = section252 * (2 | 0) | 0;
      value = $value;
    }
    return value;
  };
  return $function($effect);
}
export const hasNext = $foreign["hasNext"];
export const next = $foreign["next"];
export const drain = /* @__PURE__ */ (() => {
  return () => {
    while (hasNext()) {
      next();
    }
    return;
  };
})();
