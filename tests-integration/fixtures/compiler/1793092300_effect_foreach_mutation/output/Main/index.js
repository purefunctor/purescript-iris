import * as Control_Monad_ST_Internal from "../Control.Monad.ST.Internal/index.js";
import * as Data_Functor from "../Data.Functor/index.js";
import * as Data_Unit from "../Data.Unit/index.js";
import * as $foreign from "./foreign.js";
export function visit(values) {
  const $value = [];
  return () => {
    let seen = $value;
    const $length = values.length;
    for (let $index = 0; $index < $length; $index++) {
      const value = values[$index];
      const $condition = value === (1 | 0);
      const $action = poke(values)(2 | 0)(99 | 0);
      if ($condition) {
        $action();
      }
      const $action$1 = push(values)(value);
      const $unit = $action$1();
      const section125 = seen;
      const $value$1 = [...section125, value];
      seen = $value$1;
    }
    return seen;
  };
}
export function record(seen) {
  return (value) => {
    return () => {
      const section163 = seen.value;
      const $value = [...section163, value];
      seen.value = $value;
      return Data_Unit.unit;
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
  const $function = Control_Monad_ST_Internal.run;
  return $function(() => {
    let total = 0 | 0;
    const $length = values.length;
    for (let $index = 0; $index < $length; $index++) {
      const value = values[$index];
      const $function$1 = Data_Functor.void(Control_Monad_ST_Internal.functorST);
      (/* @__PURE__ */ $function$1(() => {
        const section256 = total;
        const $value = section256 + value | 0;
        return total = $value;
      }))();
    }
    return total;
  });
}
export const push = $foreign["push"];
export const poke = $foreign["poke"];
