import * as Data_Eq from "../Data.Eq/index.js";
import * as Data_Ord from "../Data.Ord/index.js";
import * as Data_Ordering from "../Data.Ordering/index.js";
export function eqAppType(eq1FDict) {
  return (eqADict) => {
    return { eq: (left) => {
      return (right) => {
        const left0 = left;
        const right0 = right;
        if (/* @__PURE__ */ Data_Eq.eq1(eq1FDict)(eqADict)(left0)(right0)) {
          return true;
        } else {
          return false;
        }
      };
    } };
  };
}
export function eq1AppType(eq1FDict) {
  return { eq1: (eqADict) => /* @__PURE__ */ Data_Eq.eq(/* @__PURE__ */ eqAppType(eq1FDict)(eqADict)) };
}
export function ordAppType(ord1FDict) {
  return (ordADict) => {
    return {
      Eq0: () => /* @__PURE__ */ eqAppType(/* @__PURE__ */ ord1FDict.Eq10())(/* @__PURE__ */ ordADict.Eq0()),
      compare: (left) => {
        return (right) => {
          const left0 = left;
          const right0 = right;
          const $scrutinee = /* @__PURE__ */ Data_Ord.compare1(ord1FDict)(ordADict)(left0)(right0);
          if ($scrutinee === "EQ") {
            return "EQ";
          }
          const ordering = $scrutinee;
          return ordering;
        };
      }
    };
  };
}
export function ord1AppType(ord1FDict) {
  return {
    Eq10: () => /* @__PURE__ */ eq1AppType(/* @__PURE__ */ ord1FDict.Eq10()),
    compare1: (ordADict) => /* @__PURE__ */ Data_Ord.compare(/* @__PURE__ */ ordAppType(ord1FDict)(ordADict))
  };
}
export const newtypeApp = { Coercible0: () => ({}) };
