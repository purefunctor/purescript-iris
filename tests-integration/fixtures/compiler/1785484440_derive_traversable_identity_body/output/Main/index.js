import * as Data_Functor from "../Data.Functor/index.js";
import * as Data_Traversable from "../Data.Traversable/index.js";
import * as $runtime from "../runtime.js";
export const Identity = ($value0) => ({
  tag: "Identity",
  _1: $value0
});
const $lazy_traversableIdentity = $runtime.binding("traversableIdentity", () => {
  return {
    Functor0: () => functorIdentity,
    Foldable1: () => foldableIdentity,
    traverse: (applicativeMDict) => {
      return ($function) => {
        return (value) => {
          if (value.tag === "Identity") {
            const { _1: field0 } = value;
            return /* @__PURE__ */ Data_Functor.map(/* @__PURE__ */ (/* @__PURE__ */ applicativeMDict.Apply0()).Functor0())((field0Result) => ({
              tag: "Identity",
              _1: field0Result
            }))($function(field0));
          }
          throw new Error("Pattern match failure");
        };
      };
    },
    sequence: (applicativeMDict$1) => (value$1) => /* @__PURE__ */ Data_Traversable.traverse($lazy_traversableIdentity())(applicativeMDict$1)((effect0) => effect0)(value$1)
  };
});
export const functorIdentity = /* @__PURE__ */ (() => {
  return { map: ($function) => {
    return (value) => {
      if (value.tag === "Identity") {
        const { _1: field0 } = value;
        return {
          tag: "Identity",
          _1: $function(field0)
        };
      }
      throw new Error("Pattern match failure");
    };
  } };
})();
export const foldableIdentity = /* @__PURE__ */ (() => {
  return {
    foldr: ($function) => {
      return (accumulator) => {
        return (value) => {
          if (value.tag === "Identity") {
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
          if (value$1.tag === "Identity") {
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
          if (value$2.tag === "Identity") {
            const { _1: field0$2 } = value$2;
            return $function$2(field0$2);
          }
          throw new Error("Pattern match failure");
        };
      };
    }
  };
})();
export const traversableIdentity = $lazy_traversableIdentity();
