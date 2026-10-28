import * as Data_Unit from "../Data.Unit/index.js";
export function captured(flag) {
  return () => {
    let ref = 0 | 0;
    if (flag) {
      ref = ref + (5 | 0) | 0;
    }
    if (flag) {
      ref = ref * (2 | 0) | 0;
    }
    const $effect = () => {
      ref = ref + (1 | 0) | 0;
    };
    const increment = $effect;
    const $unit = increment();
    const $unit$1 = increment();
    return ref;
  };
}
export const capturedAction = /* @__PURE__ */ (() => {
  return () => {
    let ref = 0 | 0;
    const $effect = () => {
      return ref = ref + (1 | 0) | 0;
    };
    const bump = $effect;
    const $int = bump();
    return bump;
  };
})();
export const counter = /* @__PURE__ */ (() => {
  return () => {
    let ref = 0 | 0;
    const $effect = () => {
      ref = ref + (1 | 0) | 0;
    };
    const increment = $effect;
    const $unit = increment();
    const $effect$1 = () => {
      return ref;
    };
    const $value = {
      increment,
      current: $effect$1
    };
    return $value;
  };
})();
