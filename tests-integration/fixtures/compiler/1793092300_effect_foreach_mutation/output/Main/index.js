import * as Control_Applicative from "../Control.Applicative/index.js";
import * as Control_Monad_ST_Internal from "../Control.Monad.ST.Internal/index.js";
import * as Data_Functor from "../Data.Functor/index.js";
import * as Effect from "../Effect/index.js";
import * as $foreign from "./foreign.js";
export function visit(values) {
  const $value = [];
  return () => {
    let seen = $value;
    const $action$2 = Effect.foreachE(values)((value) => {
      const $action = /* @__PURE__ */ Control_Applicative.when(Effect.applicativeEffect)(value === (1 | 0))(poke(values)(2 | 0)(99 | 0));
      return () => {
        const $unit = $action();
        const $action$1 = push(values)(value);
        const $unit$1 = $action$1();
        const section125 = seen;
        const $value$1 = [...section125, value];
        seen = $value$1;
        return undefined;
      };
    });
    const $unit$2 = $action$2();
    return seen;
  };
}
export function record(seen) {
  return (value) => {
    return () => {
      const section163 = seen.value;
      const $value = [...section163, value];
      seen.value = $value;
      return undefined;
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
    let total = 0 | 0;
    const $action = Control_Monad_ST_Internal.foreach(values)((value) => {
      const $function$1 = Data_Functor.void(Control_Monad_ST_Internal.functorST);
      const $effect$1 = () => {
        const section256 = total;
        const $value = section256 + value | 0;
        return total = $value;
      };
      return /* @__PURE__ */ $function$1($effect$1);
    });
    const $unit = $action();
    return total;
  };
  return $function($effect);
}
export const push = $foreign["push"];
export const poke = $foreign["poke"];
