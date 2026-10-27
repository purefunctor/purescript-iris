import * as Control_Monad_ST_Internal from "../Control.Monad.ST.Internal/index.js";
import * as Effect from "../Effect/index.js";
import * as $foreign from "./foreign.js";
export function sumRange(start) {
  return (end) => {
    return () => {
      const total = { value: 0 | 0 };
      const $closure = (index) => {
        return () => {
          total.value = total.value + index | 0;
        };
      };
      const $action = Effect.forE(traced("start")(start))(traced("end")(end))($closure);
      const $unit = $action();
      return total.value;
    };
  };
}
export function addTo(total) {
  return (index) => {
    return () => {
      total.value = total.value + index | 0;
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
    const total = { value: 1 | 0 };
    const $closure = (index) => {
      return () => {
        return total.value = total.value * index | 0;
      };
    };
    const $action = Control_Monad_ST_Internal.for(1 | 0)(end)($closure);
    const $unit = $action();
    return total.value;
  };
  return $function($effect);
}
export const traced = $foreign["traced"];
