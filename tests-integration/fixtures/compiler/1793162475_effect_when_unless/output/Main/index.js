import * as Control_Applicative from "../Control.Applicative/index.js";
import * as Control_Monad_ST_Internal from "../Control.Monad.ST.Internal/index.js";
import * as Effect from "../Effect/index.js";
import * as $foreign from "./foreign.js";
export function statementWhen(flag) {
  return () => {
    let ref = 0 | 0;
    const $function = Control_Applicative.when(Effect.applicativeEffect)(flag);
    const $effect = () => {
      ref = 5 | 0;
    };
    const $action = /* @__PURE__ */ $function($effect);
    const $unit = $action();
    const $function$1 = Control_Applicative.unless(Effect.applicativeEffect)(flag);
    const $effect$1 = () => {
      ref = ref + (1 | 0) | 0;
    };
    const $action$1 = /* @__PURE__ */ $function$1($effect$1);
    const $unit$1 = $action$1();
    return ref;
  };
}
export function finalWhen(flag) {
  const $action = record("before");
  return () => {
    const $unit = $action();
    return (/* @__PURE__ */ Control_Applicative.when(Effect.applicativeEffect)(flag)(record("inside")))();
  };
}
export function stateUnless(count) {
  let ref = count;
  const $function = Control_Applicative.unless(Control_Monad_ST_Internal.applicativeST)(count > (0 | 0));
  const $effect = () => {
    ref = 0 | 0;
  };
  const $action = /* @__PURE__ */ $function($effect);
  const $unit = $action();
  return ref;
}
export const record = $foreign["record"];
export const takeLog = $foreign["takeLog"];
