import * as Control_Monad_ST_Internal from "../Control.Monad.ST.Internal/index.js";
import * as Effect from "../Effect/index.js";
export function elementClosures(values) {
  const $value = [];
  return () => {
    let closures = $value;
    const $action = Effect.foreachE(values)((value) => {
      return () => {
        const section127 = closures;
        const $value$1 = [...section127, ($unit) => value];
        closures = $value$1;
        return undefined;
      };
    });
    const $unit$1 = $action();
    return closures;
  };
}
export const rangeClosures = /* @__PURE__ */ (() => {
  const $value = [];
  return () => {
    let closures = $value;
    const $action = Effect.forE(0 | 0)(3 | 0)((index) => {
      return () => {
        const section68 = closures;
        const $value$1 = [...section68, ($unit) => index];
        closures = $value$1;
        return undefined;
      };
    });
    const $unit$1 = $action();
    return closures;
  };
})();
export const stateClosures = /* @__PURE__ */ (() => {
  const $function = Control_Monad_ST_Internal.run;
  const $value = [];
  const $effect = () => {
    let closures = $value;
    const $action = Control_Monad_ST_Internal.for(5 | 0)(8 | 0)((index) => {
      return () => {
        const section184 = closures;
        const $value$1 = [...section184, ($unit) => index];
        return closures = $value$1;
      };
    });
    const $unit$1 = $action();
    return closures;
  };
  return $function($effect);
})();
