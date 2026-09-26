import * as Control_Bind from "../Control.Bind/index.js";
import * as Effect from "../Effect/index.js";
import * as $foreign from "./foreign.js";
export function genericBind(bindMDict) {
  return /* @__PURE__ */ Control_Bind.bind(bindMDict);
}
export function aliased(seed) {
  return /* @__PURE__ */ genericBind(Effect.bindEffect)(constructEffect("alias-first")(seed))((value) => constructEffect("alias-second")(value));
}
export const constructEffect = $foreign["constructEffect"];
export const mark = $foreign["mark"];
export const deferredValue = mark("deferred-value")("deferred");
export const deferredEffect = /* @__PURE__ */ (() => {
  const $action = constructEffect("deferred-action")("ignored");
  return () => {
    const value = $action();
    return constructEffect("deferred-result")(deferredValue)();
  };
})();
