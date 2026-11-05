import * as Data_Eq from "../Data.Eq/index.js";
import * as Data_Ord from "../Data.Ord/index.js";
import * as Data_Ordering from "../Data.Ordering/index.js";
export const Wrap = ($value0) => ({
  tag: "Wrap",
  _1: $value0
});
export function eqWrapType(eq1FDict) {
  return (eqADict) => {
    return { eq: (left) => {
      return (right) => {
        if (left.tag === "Wrap" && right.tag === "Wrap") {
          const { _1: left0 } = left;
          const { _1: right0 } = right;
          if (/* @__PURE__ */ Data_Eq.eq1(eq1FDict)(eqADict)(left0)(right0)) {
            return true;
          } else {
            return false;
          }
        }
        throw new Error("Pattern match failure");
      };
    } };
  };
}
export function eq1WrapType(eq1FDict) {
  return { eq1: (eqADict) => /* @__PURE__ */ Data_Eq.eq(/* @__PURE__ */ eqWrapType(eq1FDict)(eqADict)) };
}
export function ordWrapType(ord1FDict) {
  return (ordADict) => {
    return {
      Eq0: () => /* @__PURE__ */ eqWrapType(/* @__PURE__ */ ord1FDict.Eq10())(/* @__PURE__ */ ordADict.Eq0()),
      compare: (left) => {
        return (right) => {
          if (left.tag === "Wrap" && right.tag === "Wrap") {
            const { _1: left0 } = left;
            const { _1: right0 } = right;
            const $scrutinee = /* @__PURE__ */ Data_Ord.compare1(ord1FDict)(ordADict)(left0)(right0);
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
  };
}
export function ord1WrapType(ord1FDict) {
  return {
    Eq10: () => /* @__PURE__ */ eq1WrapType(/* @__PURE__ */ ord1FDict.Eq10()),
    compare1: (ordADict) => /* @__PURE__ */ Data_Ord.compare(/* @__PURE__ */ ordWrapType(ord1FDict)(ordADict))
  };
}
