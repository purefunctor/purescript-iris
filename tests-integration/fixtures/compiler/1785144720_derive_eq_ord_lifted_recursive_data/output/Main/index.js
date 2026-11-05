import * as Data_Eq from "../Data.Eq/index.js";
import * as Data_Ord from "../Data.Ord/index.js";
import * as Data_Ordering from "../Data.Ordering/index.js";
export function eqMu(eq1FDict) {
  return { eq: (left) => {
    return (right) => {
      const left0 = left;
      const right0 = right;
      if (/* @__PURE__ */ Data_Eq.eq1(eq1FDict)(/* @__PURE__ */ eqMu(eq1FDict))(left0)(right0)) {
        return true;
      } else {
        return false;
      }
    };
  } };
}
export function ordMu(ord1FDict) {
  return {
    Eq0: () => /* @__PURE__ */ eqMu(/* @__PURE__ */ ord1FDict.Eq10()),
    compare: (left) => {
      return (right) => {
        const left0 = left;
        const right0 = right;
        const $scrutinee = /* @__PURE__ */ Data_Ord.compare1(ord1FDict)(/* @__PURE__ */ ordMu(ord1FDict))(left0)(right0);
        if ($scrutinee === "EQ") {
          return "EQ";
        }
        const ordering = $scrutinee;
        return ordering;
      };
    }
  };
}
