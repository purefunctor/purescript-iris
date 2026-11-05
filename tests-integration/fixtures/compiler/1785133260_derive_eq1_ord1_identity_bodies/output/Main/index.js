import * as Data_Eq from "../Data.Eq/index.js";
import * as Data_Ord from "../Data.Ord/index.js";
import * as Data_Ordering from "../Data.Ordering/index.js";
export const Identity = ($value0) => ({
  tag: "Identity",
  _1: $value0
});
export function eqIdentity(eqADict) {
  return { eq: (left) => {
    return (right) => {
      if (left.tag === "Identity" && right.tag === "Identity") {
        const { _1: left0 } = left;
        const { _1: right0 } = right;
        if (/* @__PURE__ */ Data_Eq.eq(eqADict)(left0)(right0)) {
          return true;
        } else {
          return false;
        }
      }
      throw new Error("Pattern match failure");
    };
  } };
}
export function ordIdentity(ordADict) {
  return {
    Eq0: () => /* @__PURE__ */ eqIdentity(/* @__PURE__ */ ordADict.Eq0()),
    compare: (left) => {
      return (right) => {
        if (left.tag === "Identity" && right.tag === "Identity") {
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
export const eq1Identity = { eq1: (eqADict) => /* @__PURE__ */ Data_Eq.eq(/* @__PURE__ */ eqIdentity(eqADict)) };
export const ord1Identity = {
  Eq10: () => eq1Identity,
  compare1: (ordADict) => /* @__PURE__ */ Data_Ord.compare(/* @__PURE__ */ ordIdentity(ordADict))
};
