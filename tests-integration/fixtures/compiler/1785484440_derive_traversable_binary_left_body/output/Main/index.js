import * as Control_Applicative from "../Control.Applicative/index.js";
import * as Data_Bifoldable from "../Data.Bifoldable/index.js";
import * as Data_Bifunctor from "../Data.Bifunctor/index.js";
import * as Data_Bitraversable from "../Data.Bitraversable/index.js";
import * as Data_Functor from "../Data.Functor/index.js";
import * as Data_Monoid from "../Data.Monoid/index.js";
import * as Data_Traversable from "../Data.Traversable/index.js";
export const LeftDuplicate = ($value0) => ({
  tag: "LeftDuplicate",
  _1: $value0
});
export function functorLeftDuplicateType(bifunctorPDict) {
  return { map: ($function) => {
    return ($leftDuplicate) => {
      if ($leftDuplicate.tag === "LeftDuplicate") {
        const { _1: value } = $leftDuplicate;
        return {
          tag: "LeftDuplicate",
          _1: /* @__PURE__ */ Data_Bifunctor.bimap(bifunctorPDict)($function)((fixed) => fixed)(value)
        };
      } else {
        throw new Error("Pattern match failure");
      }
    };
  } };
}
export function foldableLeftDuplicateType(bifoldablePDict) {
  return {
    foldr: ($function) => {
      return (initial) => {
        return ($leftDuplicate) => {
          if ($leftDuplicate.tag === "LeftDuplicate") {
            const { _1: value } = $leftDuplicate;
            return /* @__PURE__ */ Data_Bifoldable.bifoldr(bifoldablePDict)($function)(($int) => (accumulated) => accumulated)(initial)(value);
          } else {
            throw new Error("Pattern match failure");
          }
        };
      };
    },
    foldl: ($function$1) => {
      return (initial$1) => {
        return ($leftDuplicate$1) => {
          if ($leftDuplicate$1.tag === "LeftDuplicate") {
            const { _1: value$1 } = $leftDuplicate$1;
            return /* @__PURE__ */ Data_Bifoldable.bifoldl(bifoldablePDict)($function$1)((accumulated$1) => ($int$1) => accumulated$1)(initial$1)(value$1);
          } else {
            throw new Error("Pattern match failure");
          }
        };
      };
    },
    foldMap: (monoidMDict) => {
      return ($function$2) => {
        return ($leftDuplicate$2) => {
          if ($leftDuplicate$2.tag === "LeftDuplicate") {
            const { _1: value$2 } = $leftDuplicate$2;
            return /* @__PURE__ */ Data_Bifoldable.bifoldMap(bifoldablePDict)(monoidMDict)($function$2)(($int$2) => /* @__PURE__ */ Data_Monoid.mempty(monoidMDict))(value$2);
          } else {
            throw new Error("Pattern match failure");
          }
        };
      };
    }
  };
}
export function traversableLeftDuplicateType(bitraversablePDict) {
  return {
    Functor0: () => /* @__PURE__ */ functorLeftDuplicateType(/* @__PURE__ */ bitraversablePDict.Bifunctor0()),
    Foldable1: () => /* @__PURE__ */ foldableLeftDuplicateType(/* @__PURE__ */ bitraversablePDict.Bifoldable1()),
    traverse: (applicativeMDict) => {
      return ($function) => {
        return (value) => {
          if (value.tag === "LeftDuplicate") {
            const { _1: field0 } = value;
            return /* @__PURE__ */ Data_Functor.map(/* @__PURE__ */ (/* @__PURE__ */ applicativeMDict.Apply0()).Functor0())((field0Result) => ({
              tag: "LeftDuplicate",
              _1: field0Result
            }))(/* @__PURE__ */ Data_Bitraversable.bitraverse(bitraversablePDict)(applicativeMDict)((element) => $function(element))((unchanged) => /* @__PURE__ */ Control_Applicative.pure(applicativeMDict)(unchanged))(field0));
          }
          throw new Error("Pattern match failure");
        };
      };
    },
    sequence: (applicativeMDict$1) => (value$1) => /* @__PURE__ */ Data_Traversable.traverse(/* @__PURE__ */ traversableLeftDuplicateType(bitraversablePDict))(applicativeMDict$1)((effect0) => effect0)(value$1)
  };
}
