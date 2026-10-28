import * as Control_Bind from "../Control.Bind/index.js";
import * as Data_Traversable from "../Data.Traversable/index.js";
import * as Data_Unit from "../Data.Unit/index.js";
import * as Effect from "../Effect/index.js";
export function previous(values) {
  return () => {
    let latest = 0 | 0;
    const $value = [];
    let closures = $value;
    const $length = values.length;
    for (let $index = 0; $index < $length; $index++) {
      const value = values[$index];
      const $modified = {
        state: value,
        value: latest
      };
      latest = $modified.state;
      const old = $modified.value;
      closures = [...closures, ($unit) => old];
    }
    return closures;
  };
}
export const snapshots = /* @__PURE__ */ (() => {
  return () => {
    let counter = 0 | 0;
    const $value = [];
    let closures = $value;
    for (let $int = 0 | 0; $int < (3 | 0); $int++) {
      counter = counter + (10 | 0) | 0;
      const value = counter;
      closures = [...closures, ($unit) => value];
    }
    return closures;
  };
})();
export const chained = /* @__PURE__ */ (() => {
  const $value = ($unit) => 0 | 0;
  return () => {
    let ref = $value;
    for (let $int = 0 | 0; $int < (3 | 0); $int++) {
      const $function = ref;
      ref = ($unit$1) => $function(Data_Unit.unit) + (1 | 0) | 0;
    }
    const $function$1 = ref;
    const $value$1 = $function$1(Data_Unit.unit);
    return $value$1;
  };
})();
export const readers = /* @__PURE__ */ (() => {
  return () => {
    let counter = 0 | 0;
    const $value = [];
    let actions = $value;
    for (let index = 0 | 0; index < (3 | 0); index++) {
      counter = index;
      const $effect = () => {
        return counter;
      };
      actions = [...actions, $effect];
    }
    counter = 42 | 0;
    const $function = Control_Bind.bind(Effect.bindEffect);
    const $effect$1 = () => {
      return actions;
    };
    return (/* @__PURE__ */ $function($effect$1)(/* @__PURE__ */ Data_Traversable.sequence(Data_Traversable.traversableArray)(Effect.applicativeEffect)))();
  };
})();
