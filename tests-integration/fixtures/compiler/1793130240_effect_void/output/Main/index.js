import * as $foreign from "./foreign.js";
export function discarded(action) {
  return () => {
    action();
  };
}
export const record = $foreign["record"];
export const takeLog = $foreign["takeLog"];
export const statements = /* @__PURE__ */ (() => {
  return () => {
    let ref = 0 | 0;
    ref = ref + (1 | 0) | 0;
    ref = ref + (2 | 0) | 0;
    return ref;
  };
})();
export const final = /* @__PURE__ */ (() => {
  const $action = record("first");
  return () => {
    const $int = $action();
    const $action$1 = record("second");
    $action$1();
  };
})();
export const nested = /* @__PURE__ */ (() => {
  const $action = record("inner");
  return () => {
    let $value;
    const $int = $action();
    $value = record("last")();
  };
})();
export const stateWrite = /* @__PURE__ */ (() => {
  let ref = 0 | 0;
  ref = 5 | 0;
})();
