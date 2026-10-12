import * as Data_Show from "../Data.Show/index.js";
export function add(value) {
  return ($int) => {
    return value;
  };
}
export const unresolvedSignature = 0 | 0;
export const usesUnresolvedSignature = add(unresolvedSignature)(1 | 0);
export const invalidSignature = 0 | 0;
export const usesInvalidSignature = add(invalidSignature)(1 | 0);
export const unresolvedValue = (() => {
  throw new Error("Generated code reached a source error");
})();
export const showsUnresolvedValue = (() => {
  const $function = Data_Show.show;
  let $result;
  throw new Error("Generated code reached a source error");
  const $function$1 = $function($result);
  let $result$1;
  throw new Error("Generated code reached a source error");
  return /* @__PURE__ */ $function$1($result$1);
})();
