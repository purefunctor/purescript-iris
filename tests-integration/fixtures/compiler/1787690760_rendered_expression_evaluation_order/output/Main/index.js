import * as $foreign from "./foreign.js";
export function ordered(branch) {
  return (shouldThrow) => {
    return collect(observe("before")(observedRecord.value))(((value) => branch ? observe("branch-true")(value) : observe("branch-false")(value))(failAt("middle")(shouldThrow)(2 | 0)))(observe("after")(3 | 0));
  };
}
export const collect = $foreign["collect"];
export const failAt = $foreign["failAt"];
export const observe = $foreign["observe"];
export const observedRecord = $foreign["observedRecord"];
export const reused = /* @__PURE__ */ (() => {
  const value = observe("reused")(4 | 0);
  return [value, value];
})();
