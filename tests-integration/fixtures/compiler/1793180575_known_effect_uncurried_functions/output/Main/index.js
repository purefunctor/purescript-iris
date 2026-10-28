import * as Effect_Uncurried from "../Effect.Uncurried/index.js";
export function addFive($function) {
  return (reference) => {
    return Effect_Uncurried.runEffectFn2($function)(reference)(5 | 0);
  };
}
export function logTwice(log) {
  return (message) => {
    const $action = Effect_Uncurried.runEffectFn1(log)(message);
    return () => {
      const $unit = $action();
      return Effect_Uncurried.runEffectFn1(log)(message + "!")();
    };
  };
}
export const addToReference = /* @__PURE__ */ (() => {
  const $closure = (reference) => {
    return (amount) => {
      return () => {
        const section56 = reference.value;
        const $value = section56 + amount | 0;
        return reference.value = $value;
      };
    };
  };
  return Effect_Uncurried.mkEffectFn2($closure);
})();
