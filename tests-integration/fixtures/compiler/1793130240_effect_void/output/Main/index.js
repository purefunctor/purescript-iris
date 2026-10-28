import * as Control_Monad_ST_Internal from "../Control.Monad.ST.Internal/index.js";
import * as Data_Functor from "../Data.Functor/index.js";
import * as Effect from "../Effect/index.js";
import * as $foreign from "./foreign.js";
export function discarded(action) {
  return /* @__PURE__ */ Data_Functor.void(Effect.functorEffect)(action);
}
export const record = $foreign["record"];
export const takeLog = $foreign["takeLog"];
export const statements = /* @__PURE__ */ (() => {
  return () => {
    const ref = { value: 0 | 0 };
    const $function = Data_Functor.void(Effect.functorEffect);
    const $effect = () => {
      return ref.value = ref.value + (1 | 0) | 0;
    };
    const $action = /* @__PURE__ */ $function($effect);
    const $unit = $action();
    const $function$1 = Data_Functor.void(Effect.functorEffect);
    const $effect$1 = () => {
      return ref.value = ref.value + (2 | 0) | 0;
    };
    const $action$1 = /* @__PURE__ */ $function$1($effect$1);
    const $unit$1 = $action$1();
    return ref.value;
  };
})();
export const final = /* @__PURE__ */ (() => {
  const $action = record("first");
  return () => {
    const $int = $action();
    return (/* @__PURE__ */ Data_Functor.void(Effect.functorEffect)(record("second")))();
  };
})();
export const nested = /* @__PURE__ */ (() => {
  const $function = Data_Functor.void(Effect.functorEffect);
  const $action = record("inner");
  const $effect = () => {
    const $int = $action();
    return record("last")();
  };
  return /* @__PURE__ */ $function($effect);
})();
export const stateWrite = /* @__PURE__ */ (() => {
  const ref = { value: 0 | 0 };
  const $function = Data_Functor.void(Control_Monad_ST_Internal.functorST);
  const $effect = () => {
    return ref.value = 5 | 0;
  };
  return (/* @__PURE__ */ $function($effect))();
})();
