import * as Control_Applicative from "../Control.Applicative/index.js";
import * as Control_Monad_ST_Internal from "../Control.Monad.ST.Internal/index.js";
import * as Data_Functor from "../Data.Functor/index.js";
import * as Effect from "../Effect/index.js";
import * as $foreign from "./foreign.js";
export function visit(values) {
  const $value = [];
  return () => {
    const seen = { value: $value };
    const $closure = (value) => {
      const $action = /* @__PURE__ */ Control_Applicative.when(Effect.applicativeEffect)(value === (1 | 0))(poke(values)(2 | 0)(99 | 0));
      return () => {
        const $unit = $action();
        const $action$1 = push(values)(value);
        const $unit$1 = $action$1();
        seen.value = [...seen.value, value];
      };
    };
    const $action$2 = Effect.foreachE(values)($closure);
    const $unit$2 = $action$2();
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
    const $action = Effect.foreachE(values)(record(seen));
    const $unit = $action();
    return seen.value;
  };
}
export function sumState(values) {
  const $function = Control_Monad_ST_Internal.run;
  const $effect = () => {
    const total = { value: 0 | 0 };
    const $closure = (value) => {
      const $function$1 = Data_Functor.void(Control_Monad_ST_Internal.functorST);
      const $effect$1 = () => {
        return total.value = total.value + value | 0;
      };
      return /* @__PURE__ */ $function$1($effect$1);
    };
    const $action = Control_Monad_ST_Internal.foreach(values)($closure);
    const $unit = $action();
    return total.value;
  };
  return $function($effect);
}
export const push = $foreign["push"];
export const poke = $foreign["poke"];
