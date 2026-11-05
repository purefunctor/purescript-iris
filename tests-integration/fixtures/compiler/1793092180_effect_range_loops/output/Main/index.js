import * as Control_Monad_ST_Internal from "../Control.Monad.ST.Internal/index.js";
import * as Effect from "../Effect/index.js";
import * as $foreign from "./foreign.js";
export function sumRange(start) {
  return (end) => {
    return () => {
      let total = 0 | 0;
      const $action = Effect.forE(traced("start")(start))(traced("end")(end))((index) => {
        return () => {
          const section91 = total;
          const $value = section91 + index | 0;
          total = $value;
          return undefined;
        };
      });
      const $unit = $action();
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
    const $action = Effect.forE(0 | 0)(end)(addTo(total));
    const $unit = $action();
    return total.value;
  };
}
export function productRange(end) {
  const $function = Control_Monad_ST_Internal.run;
  const $effect = () => {
    let total = 1 | 0;
    const $action = Control_Monad_ST_Internal.for(1 | 0)(end)((index) => {
      return () => {
        const section210 = total;
        const $value = section210 * index | 0;
        return total = $value;
      };
    });
    const $unit = $action();
    return total;
  };
  return $function($effect);
}
export const traced = $foreign["traced"];
