import * as Control_Applicative from "../Control.Applicative/index.js";
import * as Data_Bifoldable from "../Data.Bifoldable/index.js";
import * as Data_Bifunctor from "../Data.Bifunctor/index.js";
import * as Data_Bitraversable from "../Data.Bitraversable/index.js";
import * as Data_Functor from "../Data.Functor/index.js";
import * as Data_Monoid from "../Data.Monoid/index.js";
import * as Data_Traversable from "../Data.Traversable/index.js";
export const RightNested = ($value0) => ({
  tag: "RightNested",
  _1: $value0
});
export function functorRightNestedType(bifunctorPDict) {
  return { map: ($function) => {
    return ($rightNested) => {
      if ($rightNested.tag === "RightNested") {
        const { _1: value } = $rightNested;
        return {
          tag: "RightNested",
          _1: /* @__PURE__ */ Data_Bifunctor.bimap(bifunctorPDict)((fixed) => fixed)($function)(value)
        };
      } else {
        throw new Error("Pattern match failure");
      }
    };
  } };
}
export function foldableRightNestedType(bifoldablePDict) {
  return {
    foldr: ($function) => {
      return (initial) => {
        return ($rightNested) => {
          if ($rightNested.tag === "RightNested") {
            const { _1: value } = $rightNested;
            return /* @__PURE__ */ Data_Bifoldable.bifoldr(bifoldablePDict)(($int) => (accumulated) => accumulated)($function)(initial)(value);
          } else {
            throw new Error("Pattern match failure");
          }
        };
      };
    },
    foldl: ($function$1) => {
      return (initial$1) => {
        return ($rightNested$1) => {
          if ($rightNested$1.tag === "RightNested") {
            const { _1: value$1 } = $rightNested$1;
            return /* @__PURE__ */ Data_Bifoldable.bifoldl(bifoldablePDict)((accumulated$1) => ($int$1) => accumulated$1)($function$1)(initial$1)(value$1);
          } else {
            throw new Error("Pattern match failure");
          }
        };
      };
    },
    foldMap: (monoidMDict) => {
      return ($function$2) => {
        return ($rightNested$2) => {
          if ($rightNested$2.tag === "RightNested") {
            const { _1: value$2 } = $rightNested$2;
            return /* @__PURE__ */ Data_Bifoldable.bifoldMap(bifoldablePDict)(monoidMDict)(($int$2) => /* @__PURE__ */ Data_Monoid.mempty(monoidMDict))($function$2)(value$2);
          } else {
            throw new Error("Pattern match failure");
          }
        };
      };
    }
  };
}
export function traversableRightNestedType(bitraversablePDict) {
  return {
    Functor0: () => /* @__PURE__ */ functorRightNestedType(/* @__PURE__ */ bitraversablePDict.Bifunctor0()),
    Foldable1: () => /* @__PURE__ */ foldableRightNestedType(/* @__PURE__ */ bitraversablePDict.Bifoldable1()),
    traverse: (applicativeMDict) => {
      return ($function) => {
        return (value) => {
          if (value.tag === "RightNested") {
            const { _1: field0 } = value;
            return /* @__PURE__ */ Data_Functor.map(/* @__PURE__ */ (/* @__PURE__ */ applicativeMDict.Apply0()).Functor0())((field0Result) => ({
              tag: "RightNested",
              _1: field0Result
            }))(/* @__PURE__ */ Data_Bitraversable.bitraverse(bitraversablePDict)(applicativeMDict)((unchanged) => /* @__PURE__ */ Control_Applicative.pure(applicativeMDict)(unchanged))((element) => $function(element))(field0));
          }
          throw new Error("Pattern match failure");
        };
      };
    },
    sequence: (applicativeMDict$1) => (value$1) => /* @__PURE__ */ Data_Traversable.traverse(/* @__PURE__ */ traversableRightNestedType(bitraversablePDict))(applicativeMDict$1)((effect0) => effect0)(value$1)
  };
}
