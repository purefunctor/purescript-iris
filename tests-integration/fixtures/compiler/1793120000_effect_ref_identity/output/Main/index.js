import * as Data_Array from "../Data.Array/index.js";
import * as Data_Function from "../Data.Function/index.js";
import * as Effect from "../Effect/index.js";
import * as Effect_Ref from "../Effect.Ref/index.js";
import * as $foreign from "./foreign.js";
export function allocated(count) {
  const $value = [];
  return () => {
    const references = { value: $value };
    const $action = Effect.forE(0 | 0)(count)((index) => {
      return () => {
        const reference = { value: index };
        return Effect_Ref.modify_(Data_Function.flip(Data_Array.snoc)(reference))(references)();
      };
    });
    const $unit = $action();
    return references.value;
  };
}
export function throughCallback(reference) {
  return (callback) => {
    const $action = callback(reference);
    return () => {
      const $unit = $action();
      return reference.value;
    };
  };
}
export function incrementTwice(reference) {
  const $action = Effect_Ref.modify_((section253) => section253 + (1 | 0) | 0)(reference);
  return () => {
    const $unit = $action();
    return Effect_Ref.modify_((section266) => section266 + (1 | 0) | 0)(reference)();
  };
}
export const sameReference = $foreign["sameReference"];
export const incrementForeign = $foreign["incrementForeign"];
export const fresh = /* @__PURE__ */ (() => {
  return () => {
    return { value: 0 | 0 };
  };
})();
export const pair = /* @__PURE__ */ (() => {
  return () => {
    const left = { value: 0 | 0 };
    const right = { value: 0 | 0 };
    left.value = 1 | 0;
    const leftValue = left.value;
    const rightValue = right.value;
    return {
      same: sameReference(left)(right),
      left: leftValue,
      right: rightValue
    };
  };
})();
export const throughForeign = /* @__PURE__ */ (() => {
  return () => {
    const reference = { value: 1 | 0 };
    const $action = incrementForeign(reference);
    const $unit = $action();
    const section300 = reference.value;
    const $value = section300 * (10 | 0) | 0;
    return reference.value = $value;
  };
})();
export const shared = /* @__PURE__ */ (() => {
  return () => {
    const reference = { value: 0 | 0 };
    const holder = { counter: reference };
    const $reference = holder.counter;
    $reference.value = 5 | 0;
    const first = reference.value;
    let $int;
    const section359 = reference.value;
    const $value = section359 + (1 | 0) | 0;
    $int = reference.value = $value;
    const $reference$1 = holder.counter;
    const second = $reference$1.value;
    return {
      first,
      second
    };
  };
})();
