import * as Control_Monad_ST_Internal from "../Control.Monad.ST.Internal/index.js";
import * as Data_Functor from "../Data.Functor/index.js";
import * as $foreign from "./foreign.js";
export function statementWhen(flag) {
  return () => {
    let ref = 0 | 0;
    if (flag) {
      ref = 5 | 0;
    }
    const $condition = !flag;
    if ($condition) {
      const section87 = ref;
      const $value = section87 + (1 | 0) | 0;
      ref = $value;
    }
    return ref;
  };
}
export function finalWhen(flag) {
  const $action = record("before");
  return () => {
    const $unit = $action();
    const $action$1 = record("inside");
    if (flag) {
      $action$1();
    }
    return;
  };
}
export function stateUnless(count) {
  const $function = Control_Monad_ST_Internal.run;
  const $effect = () => {
    let ref = count;
    const $condition = !(count > (0 | 0));
    const $function$1 = Data_Functor.void(Control_Monad_ST_Internal.functorST);
    const $effect$1 = () => {
      return ref = 0 | 0;
    };
    const $action = /* @__PURE__ */ $function$1($effect$1);
    if ($condition) {
      $action();
    }
    return ref;
  };
  return $function($effect);
}
export const record = $foreign["record"];
export const takeLog = $foreign["takeLog"];
