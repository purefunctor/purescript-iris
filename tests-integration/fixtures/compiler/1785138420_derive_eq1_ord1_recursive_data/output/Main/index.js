import * as Data_Eq from "../Data.Eq/index.js";
import * as Data_Ord from "../Data.Ord/index.js";
export const Nil = "Nil";
export const Cons = ($value0) => ($value1) => ({
  tag: "Cons",
  _1: $value0,
  _2: $value1
});
export const Leaf = ($value0) => ({
  tag: "Leaf",
  _1: $value0
});
export const Branch = ($value0) => ($value1) => ({
  tag: "Branch",
  _1: $value0,
  _2: $value1
});
export function eqList(eqADict) {
  return { eq: (left) => {
    return (right) => {
      if (left === "Nil" && right === "Nil") {
        return true;
      }
      if (left.tag === "Cons" && right.tag === "Cons") {
        const { _1: left0, _2: left1 } = left;
        const { _1: right0, _2: right1 } = right;
        if (/* @__PURE__ */ Data_Eq.eq(eqADict)(left0)(right0)) {
          if (/* @__PURE__ */ Data_Eq.eq(/* @__PURE__ */ eqList(eqADict))(left1)(right1)) {
            return true;
          } else {
            return false;
          }
        } else {
          return false;
        }
      }
      return false;
    };
  } };
}
export function ordList(ordADict) {
  return {
    Eq0: () => /* @__PURE__ */ eqList(/* @__PURE__ */ ordADict.Eq0()),
    compare: (left) => {
      return (right) => {
        if (left === "Nil" && right === "Nil") {
          return "EQ";
        }
        if (left === "Nil") {
          return "LT";
        }
        if (right === "Nil") {
          return "GT";
        }
        if (left.tag === "Cons" && right.tag === "Cons") {
          const { _1: left0, _2: left1 } = left;
          const { _1: right0, _2: right1 } = right;
          const $scrutinee = /* @__PURE__ */ Data_Ord.compare(ordADict)(left0)(right0);
          if ($scrutinee === "EQ") {
            const $scrutinee$1 = /* @__PURE__ */ Data_Ord.compare(/* @__PURE__ */ ordList(ordADict))(left1)(right1);
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
}
export function eqTree(eqADict) {
  return { eq: (left) => {
    return (right) => {
      const eqTreeDict = /* @__PURE__ */ eqTree(eqADict);
      if (left.tag === "Leaf" && right.tag === "Leaf") {
        const { _1: left0 } = left;
        const { _1: right0 } = right;
        if (/* @__PURE__ */ Data_Eq.eq(eqADict)(left0)(right0)) {
          return true;
        } else {
          return false;
        }
      }
      if (left.tag === "Branch" && right.tag === "Branch") {
        const { _1: left0$1, _2: left1 } = left;
        const { _1: right0$1, _2: right1 } = right;
        if (/* @__PURE__ */ Data_Eq.eq(eqTreeDict)(left0$1)(right0$1)) {
          if (/* @__PURE__ */ Data_Eq.eq(eqTreeDict)(left1)(right1)) {
            return true;
          } else {
            return false;
          }
        } else {
          return false;
        }
      }
      return false;
    };
  } };
}
export function ordTree(ordADict) {
  return {
    Eq0: () => /* @__PURE__ */ eqTree(/* @__PURE__ */ ordADict.Eq0()),
    compare: (left) => {
      return (right) => {
        const ordTreeDict = /* @__PURE__ */ ordTree(ordADict);
        if (left.tag === "Leaf" && right.tag === "Leaf") {
          const { _1: left0 } = left;
          const { _1: right0 } = right;
          const $scrutinee = /* @__PURE__ */ Data_Ord.compare(ordADict)(left0)(right0);
          if ($scrutinee === "EQ") {
            return "EQ";
          }
          const ordering = $scrutinee;
          return ordering;
        }
        if (left.tag === "Leaf") {
          return "LT";
        }
        if (right.tag === "Leaf") {
          return "GT";
        }
        if (left.tag === "Branch" && right.tag === "Branch") {
          const { _1: left0$1, _2: left1 } = left;
          const { _1: right0$1, _2: right1 } = right;
          const $scrutinee$1 = /* @__PURE__ */ Data_Ord.compare(ordTreeDict)(left0$1)(right0$1);
          if ($scrutinee$1 === "EQ") {
            const $scrutinee$2 = /* @__PURE__ */ Data_Ord.compare(ordTreeDict)(left1)(right1);
            if ($scrutinee$2 === "EQ") {
              return "EQ";
            }
            const ordering$1 = $scrutinee$2;
            return ordering$1;
          }
          const ordering$2 = $scrutinee$1;
          return ordering$2;
        }
        throw new Error("Pattern match failure");
      };
    }
  };
}
export const eq1List = { eq1: (eqADict) => /* @__PURE__ */ Data_Eq.eq(/* @__PURE__ */ eqList(eqADict)) };
export const ord1List = {
  Eq10: () => eq1List,
  compare1: (ordADict) => /* @__PURE__ */ Data_Ord.compare(/* @__PURE__ */ ordList(ordADict))
};
export const eq1Tree = { eq1: (eqADict) => /* @__PURE__ */ Data_Eq.eq(/* @__PURE__ */ eqTree(eqADict)) };
export const ord1Tree = {
  Eq10: () => eq1Tree,
  compare1: (ordADict) => /* @__PURE__ */ Data_Ord.compare(/* @__PURE__ */ ordTree(ordADict))
};
