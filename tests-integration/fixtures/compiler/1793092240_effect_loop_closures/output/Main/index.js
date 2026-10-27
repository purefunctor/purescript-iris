import * as Control_Monad_ST_Internal from "../Control.Monad.ST.Internal/index.js";
import * as Effect from "../Effect/index.js";
export function elementClosures(values) {
  const $value = [];
  return () => {
    const closures = { value: $value };
    const $closure = (value) => {
      return () => {
        closures.value = [...closures.value, ($unit) => value];
      };
    };
    const $action = Effect.foreachE(values)($closure);
    const $unit$1 = $action();
    return closures.value;
  };
}
export const rangeClosures = /* @__PURE__ */ (() => {
  const $value = [];
  return () => {
    const closures = { value: $value };
    const $closure = (index) => {
      return () => {
        closures.value = [...closures.value, ($unit) => index];
      };
    };
    const $action = Effect.forE(0 | 0)(3 | 0)($closure);
    const $unit$1 = $action();
    return closures.value;
  };
})();
export const stateClosures = /* @__PURE__ */ (() => {
  const $function = Control_Monad_ST_Internal.run;
  const $value = [];
  const $effect = () => {
    const closures = { value: $value };
    const $closure = (index) => {
      return () => {
        return closures.value = [...closures.value, ($unit) => index];
      };
    };
    const $action = Control_Monad_ST_Internal.for(5 | 0)(8 | 0)($closure);
    const $unit$1 = $action();
    return closures.value;
  };
  return $function($effect);
})();
