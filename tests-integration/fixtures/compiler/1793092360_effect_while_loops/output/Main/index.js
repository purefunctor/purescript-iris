import * as Control_Monad_ST_Internal from "../Control.Monad.ST.Internal/index.js";
import * as Effect from "../Effect/index.js";
import * as $foreign from "./foreign.js";
export function countdown(start) {
  return () => {
    let counter = start;
    const $value = [];
    let seen = $value;
    const $function = Effect.whileE;
    const $function$1 = (section77) => section77 > (0 | 0);
    const $effect = () => {
      return $function$1(counter);
    };
    const $function$2 = $function($effect);
    const $effect$1 = () => {
      const current = counter;
      let $unit;
      const section105 = seen;
      const $value$1 = [...section105, current];
      seen = $value$1;
      $unit = undefined;
      const $value$2 = current - (1 | 0) | 0;
      counter = $value$2;
      return undefined;
    };
    const $action = $function$2($effect$1);
    const $unit$1 = $action();
    return seen;
  };
}
export function countUntil(limit) {
  return () => {
    let counter = 0 | 0;
    const $function = Effect.untilE;
    const $effect = () => {
      let count;
      const section165 = counter;
      const $value = section165 + (1 | 0) | 0;
      count = counter = $value;
      return count >= limit;
    };
    const $action = $function($effect);
    const $unit = $action();
    return counter;
  };
}
export function doubleUntil(limit) {
  const $function = Control_Monad_ST_Internal.run;
  const $effect = () => {
    let value = 1 | 0;
    const $function$1 = Control_Monad_ST_Internal.while;
    const $function$2 = (section233) => section233 < limit;
    const $effect$1 = () => {
      return $function$2(value);
    };
    const $function$3 = $function$1($effect$1);
    const $effect$2 = () => {
      const section252 = value;
      const $value = section252 * (2 | 0) | 0;
      return value = $value;
    };
    const $action = $function$3($effect$2);
    const $unit = $action();
    return value;
  };
  return $function($effect);
}
export const hasNext = $foreign["hasNext"];
export const next = $foreign["next"];
export const drain = Effect.whileE(hasNext)(next);
