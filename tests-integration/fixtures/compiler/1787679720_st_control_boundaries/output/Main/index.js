import * as Control_Bind from "../Control.Bind/index.js";
import * as Control_Monad_ST_Internal from "../Control.Monad.ST.Internal/index.js";
import * as $foreign from "./foreign.js";
export function branched(choose) {
  return (seed) => {
    const $action = constructST("branch-action")(seed);
    return () => {
      const value = $action();
      if (choose) {
        return constructST("branch-then")(value)();
      } else {
        return constructST("branch-else")(value)();
      }
    };
  };
}
export function patternLet(seed) {
  const $action = constructST("pattern-action")(seed);
  return () => {
    const value = $action();
    const $scrutinee = { selected: value };
    const selected = $scrutinee.selected;
    return constructST("pattern-result")(selected)();
  };
}
export function genericBind(bindMDict) {
  return /* @__PURE__ */ Control_Bind.bind(bindMDict);
}
export function aliased(seed) {
  return /* @__PURE__ */ genericBind(Control_Monad_ST_Internal.bindST)(constructST("alias-first")(seed))((value) => constructST("alias-second")(value));
}
export const constructST = $foreign["constructST"];
export const mark = $foreign["mark"];
export const deferredValue = mark("deferred-value")("deferred");
export const deferredST = /* @__PURE__ */ (() => {
  const $action = constructST("deferred-action")("ignored");
  return () => {
    const value = $action();
    return constructST("deferred-result")(deferredValue)();
  };
})();
