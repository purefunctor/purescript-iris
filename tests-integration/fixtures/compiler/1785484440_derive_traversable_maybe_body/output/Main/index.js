import * as Control_Applicative from "../Control.Applicative/index.js";
import * as Data_Functor from "../Data.Functor/index.js";
import * as Data_Monoid from "../Data.Monoid/index.js";
import * as Data_Traversable from "../Data.Traversable/index.js";
import * as $runtime from "../runtime.js";
export const Nothing = "Nothing";
export const Just = ($value0) => ({
  tag: "Just",
  _1: $value0
});
const $lazy_traversableMaybe = $runtime.binding("traversableMaybe", () => {
  return {
    Functor0: () => functorMaybe,
    Foldable1: () => foldableMaybe,
    traverse: (applicativeMDict) => {
      return ($function) => {
        return (value) => {
          if (value === "Nothing") {
            return /* @__PURE__ */ Control_Applicative.pure(applicativeMDict)("Nothing");
          }
          if (value.tag === "Just") {
            const { _1: field0 } = value;
            return /* @__PURE__ */ Data_Functor.map(/* @__PURE__ */ (/* @__PURE__ */ applicativeMDict.Apply0()).Functor0())((field0Result) => ({
              tag: "Just",
              _1: field0Result
            }))($function(field0));
          }
          throw new Error("Pattern match failure");
        };
      };
    },
    sequence: (applicativeMDict$1) => (value$1) => /* @__PURE__ */ Data_Traversable.traverse($lazy_traversableMaybe())(applicativeMDict$1)((effect0) => effect0)(value$1)
  };
});
export const functorMaybe = /* @__PURE__ */ (() => {
  return { map: ($function) => {
    return (value) => {
      if (value === "Nothing") {
        return "Nothing";
      }
      if (value.tag === "Just") {
        const { _1: field0 } = value;
        return {
          tag: "Just",
          _1: $function(field0)
        };
      }
      throw new Error("Pattern match failure");
    };
  } };
})();
export const foldableMaybe = /* @__PURE__ */ (() => {
  return {
    foldr: ($function) => {
      return (accumulator) => {
        return (value) => {
          if (value === "Nothing") {
            return accumulator;
          }
          if (value.tag === "Just") {
            const { _1: field0 } = value;
            return $function(field0)(accumulator);
          }
          throw new Error("Pattern match failure");
        };
      };
    },
    foldl: ($function$1) => {
      return (accumulator$1) => {
        return (value$1) => {
          if (value$1 === "Nothing") {
            return accumulator$1;
          }
          if (value$1.tag === "Just") {
            const { _1: field0$1 } = value$1;
            return $function$1(accumulator$1)(field0$1);
          }
          throw new Error("Pattern match failure");
        };
      };
    },
    foldMap: (monoidMDict) => {
      return ($function$2) => {
        return (value$2) => {
          if (value$2 === "Nothing") {
            return /* @__PURE__ */ Data_Monoid.mempty(monoidMDict);
          }
          if (value$2.tag === "Just") {
            const { _1: field0$2 } = value$2;
            return $function$2(field0$2);
          }
          throw new Error("Pattern match failure");
        };
      };
    }
  };
})();
export const traversableMaybe = $lazy_traversableMaybe();
