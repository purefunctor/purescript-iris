import * as Effect_Ref from "../Effect.Ref/index.js";
import * as $foreign from "./foreign.js";
export function writeArgument(reference) {
  const $value = mark("write argument")(5 | 0);
  return () => {
    reference.value = $value;
    const modifyFunction = mark("modify function")((section94) => section94 + (1 | 0) | 0);
    const $effect = () => {
      const value = reference.value;
      const $value$1 = modifyFunction(value);
      return reference.value = $value$1;
    };
    const $result = $effect;
    const $int = $result();
    return reference.value;
  };
}
export function readLater(reference) {
  return () => {
    return reference.value;
  };
}
export function reusedRead(reference) {
  const $effect = () => {
    return reference.value;
  };
  const read = $effect;
  return () => {
    const before = read();
    reference.value = 7 | 0;
    const after = read();
    return {
      before,
      after
    };
  };
}
export function modifyOrder(reference) {
  const $action = Effect_Ref["modify'"]((old) => ({
    state: old * (10 | 0) | 0,
    value: {
      returned: old + (1 | 0) | 0,
      seen: peek(reference)
    }
  }))(reference);
  return () => {
    const returned = $action();
    const state = reference.value;
    return {
      returned: returned.returned,
      seen: returned.seen,
      state
    };
  };
}
export function modifyResults(reference) {
  return () => {
    let modified;
    const section277 = reference.value;
    const $value = section277 + (1 | 0) | 0;
    modified = reference.value = $value;
    const $action = Effect_Ref.modify_((section291) => section291 * (2 | 0) | 0)(reference);
    const discarded = $action();
    const final = reference.value;
    return {
      modified,
      discarded,
      final
    };
  };
}
export const mark = $foreign["mark"];
export const peek = $foreign["peek"];
export const fresh = /* @__PURE__ */ (() => {
  return () => {
    return { value: 0 | 0 };
  };
})();
export const newPerExecution = /* @__PURE__ */ (() => {
  return () => {
    let reference = 0 | 0;
    const section335 = reference;
    const $value = section335 + (1 | 0) | 0;
    return reference = $value;
  };
})();
