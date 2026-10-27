import * as Control_Monad_ST_Uncurried from "../Control.Monad.ST.Uncurried/index.js";
import * as Effect_Uncurried from "../Effect.Uncurried/index.js";
import * as $foreign from "./foreign.js";
export const addEffect = $foreign["addEffect"];
export const addTo = /* @__PURE__ */ (() => {
  const $closure = (ref) => {
    return (amount) => {
      return () => {
        return ref.value = ref.value + amount | 0;
      };
    };
  };
  return Effect_Uncurried.mkEffectFn2($closure);
})();
export const called = /* @__PURE__ */ (() => {
  return () => {
    const ref = { value: 1 | 0 };
    const $action = Effect_Uncurried.runEffectFn2(addTo)(ref)(2 | 0);
    const added = $action();
    const $action$1 = Effect_Uncurried.runEffectFn2(addEffect)(3 | 0)(4 | 0);
    const external = $action$1();
    const $value = {
      added,
      external
    };
    return $value;
  };
})();
export const partiallyApplied = Effect_Uncurried.runEffectFn2(addEffect)(1 | 0);
export const multiplyBy = /* @__PURE__ */ (() => {
  const $closure = (ref) => {
    return (factor) => {
      return () => {
        return ref.value = ref.value * factor | 0;
      };
    };
  };
  return Control_Monad_ST_Uncurried.mkSTFn2($closure);
})();
export const multiplied = /* @__PURE__ */ (() => {
  const ref = { value: 6 | 0 };
  return Control_Monad_ST_Uncurried.runSTFn2(multiplyBy)(ref)(7 | 0)();
})();
