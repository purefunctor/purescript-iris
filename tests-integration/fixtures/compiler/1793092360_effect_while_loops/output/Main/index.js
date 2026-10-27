import * as Control_Monad_ST_Internal from "../Control.Monad.ST.Internal/index.js";
import * as Effect from "../Effect/index.js";
import * as $foreign from "./foreign.js";
export function countdown(start) {
  return () => {
    const counter = { value: start };
    const $value = [];
    const seen = { value: $value };
    const $function = Effect.whileE;
    const $function$1 = (section77) => section77 > (0 | 0);
    const $effect = () => {
      return $function$1(counter.value);
    };
    const $function$2 = $function($effect);
    const $effect$1 = () => {
      const current = counter.value;
      seen.value = [...seen.value, current];
      const $value$1 = current - (1 | 0) | 0;
      counter.value = $value$1;
    };
    const $action = $function$2($effect$1);
    const $unit = $action();
    return seen.value;
  };
}
export function countUntil(limit) {
  return () => {
    const counter = { value: 0 | 0 };
    const $function = Effect.untilE;
    const $effect = () => {
      const count = counter.value = counter.value + (1 | 0) | 0;
      const $value = count >= limit;
      return $value;
    };
    const $action = $function($effect);
    const $unit = $action();
    return counter.value;
  };
}
export function doubleUntil(limit) {
  const $function = Control_Monad_ST_Internal.run;
  const $effect = () => {
    const value = { value: 1 | 0 };
    const $function$1 = Control_Monad_ST_Internal.while;
    const $function$2 = (section233) => section233 < limit;
    const $effect$1 = () => {
      return $function$2(value.value);
    };
    const $function$3 = $function$1($effect$1);
    const $effect$2 = () => {
      return value.value = value.value * (2 | 0) | 0;
    };
    const $action = $function$3($effect$2);
    const $unit = $action();
    return value.value;
  };
  return $function($effect);
}
export const hasNext = $foreign["hasNext"];
export const next = $foreign["next"];
export const drain = Effect.whileE(hasNext)(next);
