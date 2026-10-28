import * as Control_Applicative from "../Control.Applicative/index.js";
import * as Data_Unit from "../Data.Unit/index.js";
import * as Effect from "../Effect/index.js";
import * as $foreign from "./foreign.js";
export function statementWhen(flag) {
  return () => {
    let ref = 0 | 0;
    if (flag) {
      ref = 5 | 0;
    }
    if (!flag) {
      ref = ref + (1 | 0) | 0;
    }
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
  if (!(count > (0 | 0))) {
    ref = 0 | 0;
  }
  return ref;
}
export const record = $foreign["record"];
export const takeLog = $foreign["takeLog"];
