import * as Control_Applicative from "../Control.Applicative/index.js";
import * as Data_Monoid from "../Data.Monoid/index.js";
import * as Data_Traversable from "../Data.Traversable/index.js";
import * as $runtime from "../runtime.js";
export const Const = ($value0) => ({
  tag: "Const",
  _1: $value0
});
const $lazy_traversableConstType = $runtime.binding("traversableConstType", () => {
  return {
    Functor0: () => functorConstType,
    Foldable1: () => foldableConstType,
    traverse: (applicativeMDict) => {
      return ($function) => {
        return (value) => {
          if (value.tag === "Const") {
            const { _1: field0 } = value;
            return /* @__PURE__ */ Control_Applicative.pure(applicativeMDict)({
              tag: "Const",
              _1: field0
            });
          }
          throw new Error("Pattern match failure");
        };
      };
    },
    sequence: (applicativeMDict$1) => (value$1) => /* @__PURE__ */ Data_Traversable.traverse($lazy_traversableConstType())(applicativeMDict$1)((effect0) => effect0)(value$1)
  };
});
export const functorConstType = /* @__PURE__ */ (() => {
  return { map: ($function) => {
    return (value) => {
      if (value.tag === "Const") {
        const { _1: field0 } = value;
        return {
          tag: "Const",
          _1: field0
        };
      }
      throw new Error("Pattern match failure");
    };
  } };
})();
export const foldableConstType = /* @__PURE__ */ (() => {
  return {
    foldr: ($function) => {
      return (accumulator) => {
        return (value) => {
          if (value.tag === "Const") {
            const { _1: field0 } = value;
            return accumulator;
          }
          throw new Error("Pattern match failure");
        };
      };
    },
    foldl: ($function$1) => {
      return (accumulator$1) => {
        return (value$1) => {
          if (value$1.tag === "Const") {
            const { _1: field0$1 } = value$1;
            return accumulator$1;
          }
          throw new Error("Pattern match failure");
        };
      };
    },
    foldMap: (monoidMDict) => {
      return ($function$2) => {
        return (value$2) => {
          if (value$2.tag === "Const") {
            const { _1: field0$2 } = value$2;
            return /* @__PURE__ */ Data_Monoid.mempty(monoidMDict);
          }
          throw new Error("Pattern match failure");
        };
      };
    }
  };
})();
export const traversableConstType = $lazy_traversableConstType();
