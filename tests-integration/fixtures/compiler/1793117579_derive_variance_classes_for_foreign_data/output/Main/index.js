import * as Data_Eq from "../Data.Eq/index.js";
export function eqOpaque(eqADict) {
  return { eq: ($opaque) => ($opaque$1) => true };
}
export const functorOpaque = /* @__PURE__ */ (() => {
  const $closure = ($function) => {
    return (value) => {
      throw new Error("Pattern match failure");
    };
  };
  return { map: $closure };
})();
export const foldableOpaque = /* @__PURE__ */ (() => {
  const $closure = ($function) => {
    return (accumulator) => {
      return (value) => {
        throw new Error("Pattern match failure");
      };
    };
  };
  const $closure$1 = ($function$1) => {
    return (accumulator$1) => {
      return (value$1) => {
        throw new Error("Pattern match failure");
      };
    };
  };
  const $closure$2 = (monoidMDict) => {
    const $closure$3 = ($function$2) => {
      return (value$2) => {
        throw new Error("Pattern match failure");
      };
    };
    return $closure$3;
  };
  return {
    foldr: $closure,
    foldl: $closure$1,
    foldMap: $closure$2
  };
})();
export const bifunctorOpaquePair = /* @__PURE__ */ (() => {
  const $closure = (firstFunction) => {
    return (secondFunction) => {
      return (value) => {
        throw new Error("Pattern match failure");
      };
    };
  };
  return { bimap: $closure };
})();
export const eq1Opaque = { eq1: (eqADict) => /* @__PURE__ */ Data_Eq.eq(/* @__PURE__ */ eqOpaque(eqADict)) };
