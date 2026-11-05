import * as Data_Eq from "../Data.Eq/index.js";
import * as Data_Ord from "../Data.Ord/index.js";
import * as Data_Ordering from "../Data.Ordering/index.js";
export const Box = ($value0) => ({
  tag: "Box",
  _1: $value0
});
export const eqBox = /* @__PURE__ */ (() => {
  return { eq: (left) => {
    return (right) => {
      if (left.tag === "Box" && right.tag === "Box") {
        const { _1: left0 } = left;
        const { _1: right0 } = right;
        if (/* @__PURE__ */ Data_Eq.eq(/* @__PURE__ */ Data_Eq.eqRec({})(/* @__PURE__ */ Data_Eq.eqRowCons(Data_Eq.eqRowNil)({})({ reflectSymbol: ($proxy) => "value" })(Data_Eq.eqInt)))(left0)(right0)) {
          return true;
        } else {
          return false;
        }
      }
      throw new Error("Pattern match failure");
    };
  } };
})();
export const ordBox = /* @__PURE__ */ (() => {
  return {
    Eq0: () => eqBox,
    compare: (left) => {
      return (right) => {
        if (left.tag === "Box" && right.tag === "Box") {
          const { _1: left0 } = left;
          const { _1: right0 } = right;
          const $scrutinee = /* @__PURE__ */ Data_Ord.compare(/* @__PURE__ */ Data_Ord.ordRecord({})(/* @__PURE__ */ Data_Ord.ordRecordCons(Data_Ord.ordRecordNil)({})({ reflectSymbol: ($proxy) => "value" })(Data_Ord.ordInt)))(left0)(right0);
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
})();
