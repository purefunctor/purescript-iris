import * as Control_Monad_ST_Internal from "../Control.Monad.ST.Internal/index.js";
import * as Data_Functor from "../Data.Functor/index.js";
export function delayedRead(value) {
  return () => {
    let reference = value;
    const $effect = () => {
      return reference;
    };
    const readLater = $effect;
    const $value = value + (6 | 0) | 0;
    reference = $value;
    return readLater();
  };
}
export function countDown(start) {
  return () => {
    let remaining = start;
    let steps = 0 | 0;
    const $function = Control_Monad_ST_Internal.while;
    const $function$1 = (section161) => section161 > (0 | 0);
    const $effect = () => {
      return $function$1(remaining);
    };
    const $function$2 = $function($effect);
    const $effect$1 = () => {
      let $int;
      const section183 = remaining;
      const $value = section183 - (1 | 0) | 0;
      $int = remaining = $value;
      const $function$3 = Data_Functor.void(Control_Monad_ST_Internal.functorST);
      const $effect$2 = () => {
        const section200 = steps;
        const $value$1 = section200 + (1 | 0) | 0;
        return steps = $value$1;
      };
      return (/* @__PURE__ */ $function$3($effect$2))();
    };
    const $action = $function$2($effect$1);
    const $unit = $action();
    return steps;
  };
}
export const closureWrites = /* @__PURE__ */ (() => {
  return () => {
    let reference = 0 | 0;
    const $effect = () => {
      const section104 = reference;
      const $value = section104 + (1 | 0) | 0;
      return reference = $value;
    };
    const increment = $effect;
    const $int = increment();
    const $int$1 = increment();
    return reference;
  };
})();
