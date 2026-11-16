import * as Control_Applicative from "../Control.Applicative/index.js";
import * as Control_Apply from "../Control.Apply/index.js";
import * as Data_Eq from "../Data.Eq/index.js";
import * as Data_Foldable from "../Data.Foldable/index.js";
import * as Data_Functor from "../Data.Functor/index.js";
import * as Data_Monoid from "../Data.Monoid/index.js";
import * as Data_Semigroup from "../Data.Semigroup/index.js";
import * as Data_Traversable from "../Data.Traversable/index.js";
import * as $runtime from "../runtime.js";
export const Nil = "Nil";
export const Cons = ($value0) => ($value1) => ({
  tag: "Cons",
  _1: $value0,
  _2: $value1
});
export const Left2 = ($value0) => ({
  tag: "Left2",
  _1: $value0
});
export const Right2 = ($value0) => ({
  tag: "Right2",
  _1: $value0
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
export const bifunctorEither2 = /* @__PURE__ */ (() => {
  return { bimap: (firstFunction) => {
    return (secondFunction) => {
      return (value) => {
        if (value.tag === "Left2") {
          const { _1: field0 } = value;
          return {
            tag: "Left2",
            _1: firstFunction(field0)
          };
        }
        if (value.tag === "Right2") {
          const { _1: field0$1 } = value;
          return {
            tag: "Right2",
            _1: secondFunction(field0$1)
          };
        }
        throw new Error("Pattern match failure");
      };
    };
  } };
})();
