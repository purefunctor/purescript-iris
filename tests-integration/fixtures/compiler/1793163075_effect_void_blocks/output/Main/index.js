import * as $foreign from "./foreign.js";
export function finalPure(count) {
  return () => {
    let ref = count;
    ref = ref + (1 | 0) | 0;
    ref = ref * (2 | 0) | 0;
    observe(count);
    return ref;
  };
}
export const observe = $foreign["observe"];
export const takeLog = $foreign["takeLog"];
export const finalRead = /* @__PURE__ */ (() => {
  return () => {
    let ref = 0 | 0;
    ref = 3 | 0;
    return ref;
  };
})();
export const finalMap = /* @__PURE__ */ (() => {
  return () => {
    let ref = 1 | 0;
    ref = 4 | 0;
    observe(ref);
    return ref;
  };
})();
