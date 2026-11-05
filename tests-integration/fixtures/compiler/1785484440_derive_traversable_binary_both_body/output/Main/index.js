import * as Data_Bifoldable from "../Data.Bifoldable/index.js";
import * as Data_Bifunctor from "../Data.Bifunctor/index.js";
import * as Data_Bitraversable from "../Data.Bitraversable/index.js";
import * as Data_Functor from "../Data.Functor/index.js";
import * as Data_Traversable from "../Data.Traversable/index.js";
export const Duplicate = ($value0) => ({
  tag: "Duplicate",
  _1: $value0
});
export function functorDuplicateType(bifunctorPDict) {
  return { map: ($function) => {
    return ($duplicate) => {
      if ($duplicate.tag === "Duplicate") {
        const { _1: value } = $duplicate;
        return {
          tag: "Duplicate",
          _1: /* @__PURE__ */ Data_Bifunctor.bimap(bifunctorPDict)($function)($function)(value)
        };
      } else {
        throw new Error("Pattern match failure");
      }
    };
  } };
}
export function foldableDuplicateType(bifoldablePDict) {
  return {
    foldr: ($function) => {
      return (initial) => {
        return ($duplicate) => {
          if ($duplicate.tag === "Duplicate") {
            const { _1: value } = $duplicate;
            return /* @__PURE__ */ Data_Bifoldable.bifoldr(bifoldablePDict)($function)($function)(initial)(value);
          } else {
            throw new Error("Pattern match failure");
          }
        };
      };
    },
    foldl: ($function$1) => {
      return (initial$1) => {
        return ($duplicate$1) => {
          if ($duplicate$1.tag === "Duplicate") {
            const { _1: value$1 } = $duplicate$1;
            return /* @__PURE__ */ Data_Bifoldable.bifoldl(bifoldablePDict)($function$1)($function$1)(initial$1)(value$1);
          } else {
            throw new Error("Pattern match failure");
          }
        };
      };
    },
    foldMap: (monoidMDict) => {
      return ($function$2) => {
        return ($duplicate$2) => {
          if ($duplicate$2.tag === "Duplicate") {
            const { _1: value$2 } = $duplicate$2;
            return /* @__PURE__ */ Data_Bifoldable.bifoldMap(bifoldablePDict)(monoidMDict)($function$2)($function$2)(value$2);
          } else {
            throw new Error("Pattern match failure");
          }
        };
      };
    }
  };
}
export function traversableDuplicateType(bitraversablePDict) {
  return {
    Functor0: () => /* @__PURE__ */ functorDuplicateType(/* @__PURE__ */ bitraversablePDict.Bifunctor0()),
    Foldable1: () => /* @__PURE__ */ foldableDuplicateType(/* @__PURE__ */ bitraversablePDict.Bifoldable1()),
    traverse: (applicativeMDict) => {
      return ($function) => {
        return (value) => {
          if (value.tag === "Duplicate") {
            const { _1: field0 } = value;
            return /* @__PURE__ */ Data_Functor.map(/* @__PURE__ */ (/* @__PURE__ */ applicativeMDict.Apply0()).Functor0())((field0Result) => ({
              tag: "Duplicate",
              _1: field0Result
            }))(/* @__PURE__ */ Data_Bitraversable.bitraverse(bitraversablePDict)(applicativeMDict)((element) => $function(element))((element$1) => $function(element$1))(field0));
          }
          throw new Error("Pattern match failure");
        };
      };
    },
    sequence: (applicativeMDict$1) => (value$1) => /* @__PURE__ */ Data_Traversable.traverse(/* @__PURE__ */ traversableDuplicateType(bitraversablePDict))(applicativeMDict$1)((effect0) => effect0)(value$1)
  };
}
