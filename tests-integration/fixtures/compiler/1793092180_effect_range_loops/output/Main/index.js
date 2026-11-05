import * as Control_Monad_ST_Internal from "../Control.Monad.ST.Internal/index.js";
import * as $foreign from "./foreign.js";
export function sumRange(start) {
  return (end) => {
    return () => {
      let total = 0 | 0;
      const $start = traced("start")(start);
      const $end = traced("end")(end);
      for (let index = $start; index < $end; index++) {
        const section91 = total;
        const $value = section91 + index | 0;
        total = $value;
      }
      return total;
    };
  };
}
export function addTo(total) {
  return (index) => {
    return () => {
      const section125 = total.value;
      const $value = section125 + index | 0;
      total.value = $value;
      return undefined;
    };
  };
}
export function sumWithFunction(end) {
  return () => {
    const total = { value: 0 | 0 };
    const $body = addTo(total);
    for (let $index = 0 | 0; $index < end; $index++) {
      $body($index)();
    }
    return total.value;
  };
}
export function productRange(end) {
  const $function = Control_Monad_ST_Internal.run;
  const $effect = () => {
    let total = 1 | 0;
    for (let index = 1 | 0; index < end; index++) {
      const section210 = total;
      const $value = section210 * index | 0;
      total = $value;
    }
    return total;
  };
  return $function($effect);
}
export const traced = $foreign["traced"];
