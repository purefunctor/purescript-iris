import * as $foreign from "./foreign.js";
export function finalPure(count) {
  return () => {
    let ref = count;
    let $value;
    ref = ref + (1 | 0) | 0;
    $value = count;
    let $value$1;
    ref = ref * (2 | 0) | 0;
    const $value$2 = observe(count);
    $value$1 = $value$2;
    return ref;
  };
}
export const observe = $foreign["observe"];
export const takeLog = $foreign["takeLog"];
export const finalRead = /* @__PURE__ */ (() => {
  return () => {
    let ref = 0 | 0;
    let $value;
    ref = 3 | 0;
    $value = ref;
    return ref;
  };
})();
export const finalMap = /* @__PURE__ */ (() => {
  return () => {
    let ref = 1 | 0;
    let $value;
    ref = 4 | 0;
    $value = observe(ref);
    return ref;
  };
})();
