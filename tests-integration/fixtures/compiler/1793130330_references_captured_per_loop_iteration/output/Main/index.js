import * as Control_Bind from "../Control.Bind/index.js";
import * as Data_Traversable from "../Data.Traversable/index.js";
import * as Effect from "../Effect/index.js";
const traversableArrayDictSequence = /* @__PURE__ */ Data_Traversable.sequence(Data_Traversable.traversableArray);
export const sharedCounter = /* @__PURE__ */ (() => {
  return () => {
    const counter = { value: 0 | 0 };
    const $value = [];
    const actions = { value: $value };
    for (let $int = 0 | 0; $int < (3 | 0); $int++) {
      const $element = actions.value;
      const $effect = () => {
        return counter.value = counter.value + (1 | 0) | 0;
      };
      actions.value = [...$element, $effect];
    }
    const $function = Control_Bind.bind(Effect.bindEffect);
    const $effect$1 = () => {
      return actions.value;
    };
    return (/* @__PURE__ */ $function($effect$1)(/* @__PURE__ */ traversableArrayDictSequence(Effect.applicativeEffect)))();
  };
})();
export const ownCounters = /* @__PURE__ */ (() => {
  const $value = [];
  return () => {
    let actions = $value;
    for (let index = 0 | 0; index < (3 | 0); index++) {
      const $value$1 = index * (10 | 0) | 0;
      const counter = { value: $value$1 };
      const $effect = () => {
        return counter.value = counter.value + (1 | 0) | 0;
      };
      actions = [...actions, $effect];
    }
    const bumps = actions;
    const $action = /* @__PURE__ */ traversableArrayDictSequence(Effect.applicativeEffect)(bumps);
    const $array = $action();
    return (/* @__PURE__ */ traversableArrayDictSequence(Effect.applicativeEffect)(bumps))();
  };
})();
