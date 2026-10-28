import * as Control_Bind from "../Control.Bind/index.js";
import * as Data_Traversable from "../Data.Traversable/index.js";
import * as Data_Unit from "../Data.Unit/index.js";
import * as Effect from "../Effect/index.js";
export function previous(values) {
  return () => {
    const latest = { value: 0 | 0 };
    const $value = [];
    const closures = { value: $value };
    const $length = values.length;
    for (let $index = 0; $index < $length; $index++) {
      const value = values[$index];
      const $modified = {
        state: value,
        value: latest.value
      };
      latest.value = $modified.state;
      const old = $modified.value;
      closures.value = [...closures.value, ($unit) => old];
    }
    return closures.value;
  };
}
export const snapshots = /* @__PURE__ */ (() => {
  return () => {
    const counter = { value: 0 | 0 };
    const $value = [];
    const closures = { value: $value };
    for (let $int = 0 | 0; $int < (3 | 0); $int++) {
      counter.value = counter.value + (10 | 0) | 0;
      const value = counter.value;
      closures.value = [...closures.value, ($unit) => value];
    }
    return closures.value;
  };
})();
export const chained = /* @__PURE__ */ (() => {
  const $value = ($unit) => 0 | 0;
  return () => {
    const ref = { value: $value };
    for (let $int = 0 | 0; $int < (3 | 0); $int++) {
      const $function = ref.value;
      ref.value = ($unit$1) => $function(Data_Unit.unit) + (1 | 0) | 0;
    }
    const $function$1 = ref.value;
    const $value$1 = $function$1(Data_Unit.unit);
    return $value$1;
  };
})();
export const readers = /* @__PURE__ */ (() => {
  return () => {
    const counter = { value: 0 | 0 };
    const $value = [];
    const actions = { value: $value };
    for (let index = 0 | 0; index < (3 | 0); index++) {
      counter.value = index;
      const $element = actions.value;
      const $effect = () => {
        return counter.value;
      };
      actions.value = [...$element, $effect];
    }
    counter.value = 42 | 0;
    const $function = Control_Bind.bind(Effect.bindEffect);
    const $effect$1 = () => {
      return actions.value;
    };
    return (/* @__PURE__ */ $function($effect$1)(/* @__PURE__ */ Data_Traversable.sequence(Data_Traversable.traversableArray)(Effect.applicativeEffect)))();
  };
})();
