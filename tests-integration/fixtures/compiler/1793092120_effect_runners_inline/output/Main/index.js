import * as Control_Monad_ST_Internal from "../Control.Monad.ST.Internal/index.js";
import * as Effect_Unsafe from "../Effect.Unsafe/index.js";
export function doubled(value) {
  const $function = Control_Monad_ST_Internal.run;
  const $effect = () => {
    const ref = { value };
    return ref.value = ref.value * (2 | 0) | 0;
  };
  return $function($effect);
}
export function offset(value) {
  const $left = 1 | 0;
  const $function = Control_Monad_ST_Internal.run;
  const $effect = () => {
    return value;
  };
  return $left + $function($effect) | 0;
}
export function performedAction(action) {
  return Effect_Unsafe.unsafePerformEffect(action);
}
export function runAction(action) {
  return Control_Monad_ST_Internal.run(action);
}
export const performed = /* @__PURE__ */ (() => {
  const $function = Effect_Unsafe.unsafePerformEffect;
  const $effect = () => {
    const ref = { value: 20 | 0 };
    return ref.value = ref.value + (1 | 0) | 0;
  };
  return $function($effect);
})();
