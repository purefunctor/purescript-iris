import * as Data_Eq from "../Data.Eq/index.js";
import * as Data_Ord from "../Data.Ord/index.js";
import * as Data_Ordering from "../Data.Ordering/index.js";
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
export function ordMaybe(ordADict) {
  return {
    Eq0: () => /* @__PURE__ */ eqMaybe(/* @__PURE__ */ ordADict.Eq0()),
    compare: (left) => {
      return (right) => {
        if (left === "Nothing" && right === "Nothing") {
          return "EQ";
        }
        if (left === "Nothing") {
          return "LT";
        }
        if (right === "Nothing") {
          return "GT";
        }
        if (left.tag === "Just" && right.tag === "Just") {
          const { _1: left0 } = left;
          const { _1: right0 } = right;
          const $scrutinee = /* @__PURE__ */ Data_Ord.compare(ordADict)(left0)(right0);
          if ($scrutinee === "EQ") {
            return "EQ";
          }
          const ordering = $scrutinee;
          return ordering;
        }
        throw new Error("Pattern match failure");
      };
    }
  };
}
