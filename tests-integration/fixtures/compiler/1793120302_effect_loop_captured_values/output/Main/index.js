import * as Effect from "../Effect/index.js";
export function untilLimit(configuration) {
  return () => {
    let counter = 0 | 0;
    const limit = configuration.limit;
    const $function = Effect.untilE;
    const $effect = () => {
      let current;
      const section63 = counter;
      const $value = section63 + (1 | 0) | 0;
      current = counter = $value;
      return current >= limit;
    };
    const $action = $function($effect);
    const $unit = $action();
    return counter;
  };
}
export function whileLimit(configuration) {
  return () => {
    let counter = 0 | 0;
    const limit = configuration.limit;
    const $function = Effect.whileE;
    const $effect = () => {
      const current = counter;
      return current < limit;
    };
    const $function$1 = $function($effect);
    const $effect$1 = () => {
      const section147 = counter;
      const $value = section147 + (1 | 0) | 0;
      counter = $value;
      return undefined;
    };
    const $action = $function$1($effect$1);
    const $unit = $action();
    return counter;
  };
}
