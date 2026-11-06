import * as Control_Monad_ST_Internal from "../Control.Monad.ST.Internal/index.js";
export function elementClosures(values) {
  const $value = [];
  return () => {
    let closures = $value;
    const $length = values.length;
    for (let $index = 0; $index < $length; $index++) {
      const value = values[$index];
      const section127 = closures;
      const $value$1 = [...section127, ($unit) => value];
      closures = $value$1;
    }
    return closures;
  };
}
export const rangeClosures = /* @__PURE__ */ (() => {
  const $value = [];
  return () => {
    let closures = $value;
    for (let index = 0 | 0; index < (3 | 0); index++) {
      const section68 = closures;
      const $value$1 = [...section68, ($unit) => index];
      closures = $value$1;
    }
    return closures;
  };
})();
export const stateClosures = /* @__PURE__ */ (() => {
  const $function = Control_Monad_ST_Internal.run;
  const $value = [];
  const $effect = () => {
    let closures = $value;
    for (let index = 5 | 0; index < (8 | 0); index++) {
      const section184 = closures;
      const $value$1 = [...section184, ($unit) => index];
      closures = $value$1;
    }
    return closures;
  };
  return $function($effect);
})();
