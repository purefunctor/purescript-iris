import * as Control_Applicative from "../Control.Applicative/index.js";
import * as Data_Unit from "../Data.Unit/index.js";
import * as Effect from "../Effect/index.js";
export function captured(flag) {
  return () => {
    let ref = 0 | 0;
    const $function = Control_Applicative.when(Effect.applicativeEffect)(flag);
    const $effect = () => {
      ref = ref + (5 | 0) | 0;
    };
    const $action = /* @__PURE__ */ $function($effect);
    const $unit = $action();
    let $result;
    if (flag) {
      $result = () => {
        ref = ref * (2 | 0) | 0;
      };
    } else {
      const $value = Data_Unit.unit;
      $result = () => {
        return $value;
      };
    }
    const $unit$1 = $result();
    const $effect$1 = () => {
      ref = ref + (1 | 0) | 0;
    };
    const increment = $effect$1;
    const $unit$2 = increment();
    const $unit$3 = increment();
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
