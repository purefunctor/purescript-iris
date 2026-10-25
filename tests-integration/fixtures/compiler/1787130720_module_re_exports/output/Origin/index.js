import * as $foreign from "./foreign.js";
export const Just = ($value0) => ({
  tag: "Just",
  _1: $value0
});
const Nothing = "Nothing";
export function visible(value) {
  return value;
}
export function append(left) {
  return ($int) => {
    return left;
  };
}
export function measure(dictionary) {
  return dictionary.measure;
}
export const foreignValue = $foreign["foreignValue"];
const hidden = 13 | 0;
const $await = 17 | 0;
export const eqOption = /* @__PURE__ */ (() => {
  const $closure = (left) => {
    return (right) => {
      if (left.tag === "Just" && right.tag === "Just") {
        const { _1: left0 } = left;
        const { _1: right0 } = right;
        if (left0 === right0) {
          return true;
        } else {
          return false;
        }
      }
      if (left === "Nothing" && right === "Nothing") {
        return true;
      }
      return false;
    };
  };
  return { eq: $closure };
})();
export const measureInt = { measure: (value) => value };
export { $await as "await" };
