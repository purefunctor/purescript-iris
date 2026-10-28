import * as Control_Applicative from "../Control.Applicative/index.js";
import * as Effect from "../Effect/index.js";
import * as $foreign from "./foreign.js";
export function visit(values) {
  const $value = [];
  return () => {
    let seen = $value;
    const $length = values.length;
    for (let $index = 0; $index < $length; $index++) {
      const value = values[$index];
      const $action = /* @__PURE__ */ Control_Applicative.when(Effect.applicativeEffect)(value === (1 | 0))(poke(values)(2 | 0)(99 | 0));
      const $unit = $action();
      const $action$1 = push(values)(value);
      const $unit$1 = $action$1();
      seen = [...seen, value];
    }
    return seen;
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
  let total = 0 | 0;
  const $length = values.length;
  for (let $index = 0; $index < $length; $index++) {
    const value = values[$index];
    total = total + value | 0;
  }
  return total;
}
export const push = $foreign["push"];
export const poke = $foreign["poke"];
