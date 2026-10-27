import * as Control_Monad_ST_Internal from "../Control.Monad.ST.Internal/index.js";
export function elementClosures(values) {
  const $value = [];
  return () => {
    const closures = { value: $value };
    const $length = values.length;
    for (let $index = 0; $index < $length; $index++) {
      const value = values[$index];
      closures.value = [...closures.value, ($unit) => value];
    }
    return closures.value;
  };
}
export const rangeClosures = /* @__PURE__ */ (() => {
  const $value = [];
  return () => {
    const closures = { value: $value };
    for (let index = 0 | 0; index < (3 | 0); index++) {
      closures.value = [...closures.value, ($unit) => index];
    }
    return closures.value;
  };
})();
export const stateClosures = /* @__PURE__ */ (() => {
  const $function = Control_Monad_ST_Internal.run;
  const $value = [];
  const $effect = () => {
    const closures = { value: $value };
    for (let index = 5 | 0; index < (8 | 0); index++) {
      closures.value = [...closures.value, ($unit) => index];
    }
    return closures.value;
  };
  return $function($effect);
})();
