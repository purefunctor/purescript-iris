import * as Data_Eq from "../Data.Eq/index.js";
import * as Data_Ord from "../Data.Ord/index.js";
export const Tuple = ($value0) => ($value1) => ({
  tag: "Tuple",
  _1: $value0,
  _2: $value1
});
export function eqTuple(eqADict) {
  return (eqBDict) => {
    return { eq: (left) => {
      return (right) => {
        if (left.tag === "Tuple" && right.tag === "Tuple") {
          const { _1: left0, _2: left1 } = left;
          const { _1: right0, _2: right1 } = right;
          if (/* @__PURE__ */ Data_Eq.eq(eqADict)(left0)(right0)) {
            if (/* @__PURE__ */ Data_Eq.eq(eqBDict)(left1)(right1)) {
              return true;
            } else {
              return false;
            }
          } else {
            return false;
          }
        }
        throw new Error("Pattern match failure");
      };
    } };
  };
}
export function ordTuple(ordADict) {
  return (ordBDict) => {
    return {
      Eq0: () => /* @__PURE__ */ eqTuple(/* @__PURE__ */ ordADict.Eq0())(/* @__PURE__ */ ordBDict.Eq0()),
      compare: (left) => {
        return (right) => {
          if (left.tag === "Tuple" && right.tag === "Tuple") {
            const { _1: left0, _2: left1 } = left;
            const { _1: right0, _2: right1 } = right;
            const $scrutinee = /* @__PURE__ */ Data_Ord.compare(ordADict)(left0)(right0);
            if ($scrutinee === "EQ") {
              const $scrutinee$1 = /* @__PURE__ */ Data_Ord.compare(ordBDict)(left1)(right1);
              if ($scrutinee$1 === "EQ") {
                return "EQ";
              }
              const ordering = $scrutinee$1;
              return ordering;
            }
            const ordering$1 = $scrutinee;
            return ordering$1;
          }
          throw new Error("Pattern match failure");
        };
      }
    };
  };
}
