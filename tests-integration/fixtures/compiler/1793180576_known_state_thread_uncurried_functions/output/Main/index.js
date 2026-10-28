import * as Control_Monad_ST_Internal from "../Control.Monad.ST.Internal/index.js";
import * as Control_Monad_ST_Uncurried from "../Control.Monad.ST.Uncurried/index.js";
export function addTwice(value) {
  const $function = Control_Monad_ST_Internal.run;
  const $effect = () => {
    const reference = { value };
    const $action = Control_Monad_ST_Uncurried.runSTFn2(addToReference)(reference)(2 | 0);
    const $int = $action();
    return Control_Monad_ST_Uncurried.runSTFn2(addToReference)(reference)(3 | 0)();
  };
  return $function($effect);
}
export const addToReference = /* @__PURE__ */ (() => {
  const $closure = (reference) => {
    return (amount) => {
      return () => {
        const section59 = reference.value;
        const $value = section59 + amount | 0;
        return reference.value = $value;
      };
    };
  };
  return Control_Monad_ST_Uncurried.mkSTFn2($closure);
})();
