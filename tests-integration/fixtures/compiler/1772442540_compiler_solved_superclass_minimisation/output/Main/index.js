import * as $foreign from "./foreign.js";
export function useMerge(mergeR1R2R3Dict) {
  return ($proxy) => ($record) => ($record$1) => unsafeCoerce(0 | 0);
}
export function useUnion(unionR1R2R3Dict) {
  return ($proxy) => ($record) => ($record$1) => unsafeCoerce(0 | 0);
}
export function testMin(mergeDict) {
  return (p) => {
    return (r1) => {
      return (r2) => {
        const $scrutinee = /* @__PURE__ */ useMerge(mergeDict)(p)(r1)(r2);
        return /* @__PURE__ */ useUnion(/* @__PURE__ */ mergeDict.Union0())(p)(r1)(r2);
      };
    };
  };
}
export const unsafeCoerce = $foreign["unsafeCoerce"];
