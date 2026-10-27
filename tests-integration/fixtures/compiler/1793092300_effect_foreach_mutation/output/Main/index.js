import * as Control_Applicative from "../Control.Applicative/index.js";
import * as Control_Monad_ST_Internal from "../Control.Monad.ST.Internal/index.js";
import * as Data_Functor from "../Data.Functor/index.js";
import * as Effect from "../Effect/index.js";
import * as $foreign from "./foreign.js";
export function visit(values) {
  const $value = [];
  return () => {
    const seen = { value: $value };
    const $length = values.length;
    for (let $index = 0; $index < $length; $index++) {
      const value = values[$index];
      const $action = /* @__PURE__ */ Control_Applicative.when(Effect.applicativeEffect)(value === (1 | 0))(poke(values)(2 | 0)(99 | 0));
      const $unit = $action();
      const $action$1 = push(values)(value);
      const $unit$1 = $action$1();
      seen.value = [...seen.value, value];
    }
    return seen.value;
  };
}
export function record(seen) {
  return (value) => {
    return () => {
      seen.value = [...seen.value, value];
    };
  };
}
export function visitWithFunction(values) {
  const $value = [];
  return () => {
    const seen = { value: $value };
    const $body = record(seen);
    const $length = values.length;
    for (let $index = 0; $index < $length; $index++) {
      $body(values[$index])();
    }
    return seen.value;
  };
}
export function sumState(values) {
  const total = { value: 0 | 0 };
  const $length = values.length;
  for (let $index = 0; $index < $length; $index++) {
    const value = values[$index];
    const $function = Data_Functor.void(Control_Monad_ST_Internal.functorST);
    const $effect = () => {
      return total.value = total.value + value | 0;
    };
    (/* @__PURE__ */ $function($effect))();
  }
  return total.value;
}
export const push = $foreign["push"];
export const poke = $foreign["poke"];
