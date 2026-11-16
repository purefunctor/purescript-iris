import * as Control_Applicative from "../Control.Applicative/index.js";
import * as Control_Apply from "../Control.Apply/index.js";
import * as Data_Eq from "../Data.Eq/index.js";
import * as Data_Foldable from "../Data.Foldable/index.js";
import * as Data_Functor from "../Data.Functor/index.js";
import * as Data_Maybe from "../Data.Maybe/index.js";
import * as Data_Monoid from "../Data.Monoid/index.js";
import * as Data_Ord from "../Data.Ord/index.js";
import * as Data_Ordering from "../Data.Ordering/index.js";
import * as Data_Semigroup from "../Data.Semigroup/index.js";
import * as Data_Show from "../Data.Show/index.js";
import * as Data_Traversable from "../Data.Traversable/index.js";
import * as $runtime from "../runtime.js";
export const Nil = "Nil";
export const Cons = ($value0) => ($value1) => ({
  tag: "Cons",
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
const $lazy_functorList = $runtime.binding("functorList", () => {
  return { map: ($function) => {
    return (value) => {
      if (value === "Nil") {
        return "Nil";
      }
      if (value.tag === "Cons") {
        const { _1: field0, _2: field1 } = value;
        return {
          tag: "Cons",
          _1: $function(field0),
          _2: /* @__PURE__ */ Data_Functor.map($lazy_functorList())((element) => $function(element))(field1)
        };
      }
      throw new Error("Pattern match failure");
    };
  } };
});
const $lazy_foldableList = $runtime.binding("foldableList", () => {
  return {
    foldr: ($function) => {
      return (accumulator) => {
        return (value) => {
          if (value === "Nil") {
            return accumulator;
          }
          if (value.tag === "Cons") {
            const { _1: field0, _2: field1 } = value;
            return $function(field0)(/* @__PURE__ */ Data_Foldable.foldr($lazy_foldableList())((element) => (accumulator$1) => $function(element)(accumulator$1))(accumulator)(field1));
          }
          throw new Error("Pattern match failure");
        };
      };
    },
    foldl: ($function$1) => {
      return (accumulator$2) => {
        return (value$1) => {
          if (value$1 === "Nil") {
            return accumulator$2;
          }
          if (value$1.tag === "Cons") {
            const { _1: field0$1, _2: field1$1 } = value$1;
            return /* @__PURE__ */ Data_Foldable.foldl($lazy_foldableList())((accumulator$3) => (element$1) => $function$1(accumulator$3)(element$1))($function$1(accumulator$2)(field0$1))(field1$1);
          }
          throw new Error("Pattern match failure");
        };
      };
    },
    foldMap: (monoidMDict) => {
      return ($function$2) => {
        return (value$2) => {
          if (value$2 === "Nil") {
            return /* @__PURE__ */ Data_Monoid.mempty(monoidMDict);
          }
          if (value$2.tag === "Cons") {
            const { _1: field0$2, _2: field1$2 } = value$2;
            return /* @__PURE__ */ Data_Semigroup.append(/* @__PURE__ */ monoidMDict.Semigroup0())($function$2(field0$2))(/* @__PURE__ */ Data_Foldable.foldMap($lazy_foldableList())(monoidMDict)((element$2) => $function$2(element$2))(field1$2));
          }
          throw new Error("Pattern match failure");
        };
      };
    }
  };
});
const $lazy_traversableList = $runtime.binding("traversableList", () => {
  return {
    Functor0: () => $lazy_functorList(),
    Foldable1: () => $lazy_foldableList(),
    traverse: (applicativeMDict) => {
      return ($function) => {
        return (value) => {
          const Apply0Dict = /* @__PURE__ */ applicativeMDict.Apply0();
          if (value === "Nil") {
            return /* @__PURE__ */ Control_Applicative.pure(applicativeMDict)("Nil");
          }
          if (value.tag === "Cons") {
            const { _1: field0, _2: field1 } = value;
            return /* @__PURE__ */ Control_Apply.apply(Apply0Dict)(/* @__PURE__ */ Data_Functor.map(/* @__PURE__ */ Apply0Dict.Functor0())((field0Result) => (field1Result) => ({
              tag: "Cons",
              _1: field0Result,
              _2: field1Result
            }))($function(field0)))(/* @__PURE__ */ Data_Traversable.traverse($lazy_traversableList())(applicativeMDict)((element) => $function(element))(field1));
          }
          throw new Error("Pattern match failure");
        };
      };
    },
    sequence: (applicativeMDict$1) => (value$1) => /* @__PURE__ */ Data_Traversable.traverse($lazy_traversableList())(applicativeMDict$1)((effect0) => effect0)(value$1)
  };
});
export const functorList = $lazy_functorList();
export const foldableList = $lazy_foldableList();
export const traversableList = $lazy_traversableList();
export const eq1List = { eq1: (eqADict) => /* @__PURE__ */ Data_Eq.eq(/* @__PURE__ */ eqList(eqADict)) };
export const ord1List = {
  Eq10: () => eq1List,
  compare1: (ordADict) => /* @__PURE__ */ Data_Ord.compare(/* @__PURE__ */ ordList(ordADict))
};
export const mapped = /* @__PURE__ */ Data_Eq.eq(/* @__PURE__ */ eqList(Data_Eq.eqInt))(/* @__PURE__ */ Data_Functor.map($lazy_functorList())((section82) => section82 + (4 | 0) | 0)({
  tag: "Cons",
  _1: 3 | 0,
  _2: {
    tag: "Cons",
    _1: 8 | 0,
    _2: "Nil"
  }
}))({
  tag: "Cons",
  _1: 7 | 0,
  _2: {
    tag: "Cons",
    _1: 12 | 0,
    _2: "Nil"
  }
});
export const folded = /* @__PURE__ */ Data_Foldable.foldMap($lazy_foldableList())(Data_Monoid.monoidString)(/* @__PURE__ */ Data_Show.show(Data_Show.showInt))({
  tag: "Cons",
  _1: 3 | 0,
  _2: {
    tag: "Cons",
    _1: 8 | 0,
    _2: "Nil"
  }
});
export const traversed = /* @__PURE__ */ Data_Eq.eq(/* @__PURE__ */ Data_Maybe.eqMaybe(/* @__PURE__ */ eqList(Data_Eq.eqInt)))(/* @__PURE__ */ Data_Traversable.traverse($lazy_traversableList())(Data_Maybe.applicativeMaybe)((value) => value < (5 | 0) ? {
  tag: "Just",
  _1: value + (1 | 0) | 0
} : "Nothing")({
  tag: "Cons",
  _1: 1 | 0,
  _2: {
    tag: "Cons",
    _1: 3 | 0,
    _2: "Nil"
  }
}))({
  tag: "Just",
  _1: {
    tag: "Cons",
    _1: 2 | 0,
    _2: {
      tag: "Cons",
      _1: 4 | 0,
      _2: "Nil"
    }
  }
});
export const rejected = /* @__PURE__ */ Data_Eq.eq(/* @__PURE__ */ Data_Maybe.eqMaybe(/* @__PURE__ */ eqList(Data_Eq.eqInt)))(/* @__PURE__ */ Data_Traversable.traverse($lazy_traversableList())(Data_Maybe.applicativeMaybe)((value) => value < (5 | 0) ? {
  tag: "Just",
  _1: value
} : "Nothing")({
  tag: "Cons",
  _1: 1 | 0,
  _2: {
    tag: "Cons",
    _1: 8 | 0,
    _2: "Nil"
  }
}))("Nothing");
export const equal = /* @__PURE__ */ Data_Eq.eq1(eq1List)(Data_Eq.eqInt)({
  tag: "Cons",
  _1: 1 | 0,
  _2: "Nil"
})({
  tag: "Cons",
  _1: 1 | 0,
  _2: "Nil"
});
export const unequal = /* @__PURE__ */ Data_Eq.eq1(eq1List)(Data_Eq.eqInt)({
  tag: "Cons",
  _1: 1 | 0,
  _2: "Nil"
})({
  tag: "Cons",
  _1: 2 | 0,
  _2: "Nil"
});
export const ordered = /* @__PURE__ */ Data_Eq.eq(Data_Ordering.eqOrdering)(/* @__PURE__ */ Data_Ord.compare1(ord1List)(Data_Ord.ordInt)({
  tag: "Cons",
  _1: 1 | 0,
  _2: "Nil"
})({
  tag: "Cons",
  _1: 2 | 0,
  _2: "Nil"
}))("LT");
