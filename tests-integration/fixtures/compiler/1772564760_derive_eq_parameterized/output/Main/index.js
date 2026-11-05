import * as Data_Eq from "../Data.Eq/index.js";
export const Nothing = "Nothing";
export const Just = ($value0) => ({
  tag: "Just",
  _1: $value0
});
export function eqMaybe(eqADict) {
  return { eq: (left) => {
    return (right) => {
      if (left === "Nothing" && right === "Nothing") {
        return true;
      }
      if (left.tag === "Just" && right.tag === "Just") {
        const { _1: left0 } = left;
        const { _1: right0 } = right;
        if (/* @__PURE__ */ Data_Eq.eq(eqADict)(left0)(right0)) {
          return true;
        } else {
          return false;
        }
      }
      return false;
    };
  } };
}
