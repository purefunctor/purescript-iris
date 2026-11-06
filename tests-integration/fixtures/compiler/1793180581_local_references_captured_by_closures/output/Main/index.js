import * as Control_Monad_ST_Internal from "../Control.Monad.ST.Internal/index.js";
import * as Data_Functor from "../Data.Functor/index.js";
export function delayedRead(value) {
  return () => {
    let reference = value;
    const readLater = () => {
      return reference;
    };
    const $value = value + (6 | 0) | 0;
    reference = $value;
    return readLater();
  };
}
export function countDown(start) {
  return () => {
    let remaining = start;
    let steps = 0 | 0;
    const $function = (section161) => section161 > (0 | 0);
    while (true) {
      let $condition;
      $condition = $function(remaining);
      if (!$condition) {
        break;
      }
      let $int;
      const section183 = remaining;
      const $value = section183 - (1 | 0) | 0;
      $int = remaining = $value;
      const $function$1 = Data_Functor.void(Control_Monad_ST_Internal.functorST);
      (/* @__PURE__ */ $function$1(() => {
        const section200 = steps;
        const $value$1 = section200 + (1 | 0) | 0;
        return steps = $value$1;
      }))();
    }
    return steps;
  };
}
export const closureWrites = /* @__PURE__ */ (() => {
  return () => {
    let reference = 0 | 0;
    const increment = () => {
      const section104 = reference;
      const $value = section104 + (1 | 0) | 0;
      return reference = $value;
    };
    const $int = increment();
    const $int$1 = increment();
    return reference;
  };
})();
