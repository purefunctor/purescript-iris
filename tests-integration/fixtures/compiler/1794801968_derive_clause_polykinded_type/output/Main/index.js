import * as Data_Traversable from "../Data.Traversable/index.js";
import * as $runtime from "../runtime.js";
const $lazy_traversableProxyType = $runtime.binding("traversableProxyType", () => {
  return {
    Functor0: () => functorProxyType,
    Foldable1: () => foldableProxyType,
    traverse: (applicativeMDict) => {
      return ($function) => {
        return (value) => {
          throw new Error("Pattern match failure");
        };
      };
    },
    sequence: (applicativeMDict$1) => (value$1) => /* @__PURE__ */ Data_Traversable.traverse($lazy_traversableProxyType())(applicativeMDict$1)((effect0) => effect0)(value$1)
  };
});
export const functorProxyType = /* @__PURE__ */ (() => {
  return { map: ($function) => {
    return (value) => {
      throw new Error("Pattern match failure");
    };
  } };
})();
export const foldableProxyType = /* @__PURE__ */ (() => {
  return {
    foldr: ($function) => {
      return (accumulator) => {
        return (value) => {
          throw new Error("Pattern match failure");
        };
      };
    },
    foldl: ($function$1) => {
      return (accumulator$1) => {
        return (value$1) => {
          throw new Error("Pattern match failure");
        };
      };
    },
    foldMap: (monoidMDict) => {
      return ($function$2) => {
        return (value$2) => {
          throw new Error("Pattern match failure");
        };
      };
    }
  };
})();
export const traversableProxyType = $lazy_traversableProxyType();
