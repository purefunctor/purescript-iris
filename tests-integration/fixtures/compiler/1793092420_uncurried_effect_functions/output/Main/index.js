import * as Effect_Uncurried from "../Effect.Uncurried/index.js";
import * as $foreign from "./foreign.js";
export function addTo(ref, amount) {
  return ref.value = ref.value + amount | 0;
}
export function multiplyBy(ref, factor) {
  return ref.value = ref.value * factor | 0;
}
export const addEffect = $foreign["addEffect"];
export const called = /* @__PURE__ */ (() => {
  return () => {
    const ref = { value: 1 | 0 };
    const added = addTo(ref, 2 | 0);
    const external = addEffect(3 | 0, 4 | 0);
    const $value = {
      added,
      external
    };
    return $value;
  };
})();
export const partiallyApplied = Effect_Uncurried.runEffectFn2(addEffect)(1 | 0);
export const multiplied = /* @__PURE__ */ (() => {
  const ref = { value: 6 | 0 };
  return multiplyBy(ref, 7 | 0);
})();
