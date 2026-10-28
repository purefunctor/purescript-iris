import * as Control_Applicative from "../Control.Applicative/index.js";
import * as Data_Unit from "../Data.Unit/index.js";
import * as Effect from "../Effect/index.js";
export function bump(ref) {
  return () => {
    ref.value = ref.value + (1 | 0) | 0;
  };
}
export function captured(flag) {
  return () => {
    const ref = { value: 0 | 0 };
    const $function = Control_Applicative.when(Effect.applicativeEffect)(flag);
    const $effect = () => {
      ref.value = ref.value + (5 | 0) | 0;
    };
    const $action = /* @__PURE__ */ $function($effect);
    const $unit = $action();
    let $result;
    if (flag) {
      $result = () => {
        ref.value = ref.value * (2 | 0) | 0;
      };
    } else {
      const $value = Data_Unit.unit;
      $result = () => {
        return $value;
      };
    }
    const $unit$1 = $result();
    const $effect$1 = () => {
      ref.value = ref.value + (1 | 0) | 0;
    };
    const increment = $effect$1;
    const $unit$2 = increment();
    const $unit$3 = increment();
    return ref.value;
  };
}
export const passed = /* @__PURE__ */ (() => {
  return () => {
    const ref = { value: 0 | 0 };
    const $action = bump(ref);
    const $unit = $action();
    ref.value = ref.value + (10 | 0) | 0;
    return ref.value;
  };
})();
export const returned = /* @__PURE__ */ (() => {
  return () => {
    const ref = { value: 1 | 0 };
    ref.value = 2 | 0;
    return ref;
  };
})();
export const stored = /* @__PURE__ */ (() => {
  return () => {
    const ref = { value: 3 | 0 };
    ref.value = ref.value * (2 | 0) | 0;
    const $value = { counter: ref };
    return $value;
  };
})();
