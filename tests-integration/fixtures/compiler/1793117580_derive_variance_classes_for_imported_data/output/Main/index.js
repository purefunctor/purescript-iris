import * as Library from "../Library/index.js";
export const functorBox = /* @__PURE__ */ (() => {
  const $closure = ($function) => {
    return (value) => {
      if (value.tag === "Box") {
        const { _1: field0 } = value;
        return {
          tag: "Box",
          _1: $function(field0)
        };
      }
      throw new Error("Pattern match failure");
    };
  };
  return { map: $closure };
})();
export const foldableBox = /* @__PURE__ */ (() => {
  const $closure = ($function) => {
    return (accumulator) => {
      return (value) => {
        if (value.tag === "Box") {
          const { _1: field0 } = value;
          return $function(field0)(accumulator);
        }
        throw new Error("Pattern match failure");
      };
    };
  };
  const $closure$1 = ($function$1) => {
    return (accumulator$1) => {
      return (value$1) => {
        if (value$1.tag === "Box") {
          const { _1: field0$1 } = value$1;
          return $function$1(accumulator$1)(field0$1);
        }
        throw new Error("Pattern match failure");
      };
    };
  };
  const $closure$2 = (monoidMDict) => {
    const $closure$3 = ($function$2) => {
      return (value$2) => {
        if (value$2.tag === "Box") {
          const { _1: field0$2 } = value$2;
          return $function$2(field0$2);
        }
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
export const bifunctorPair = /* @__PURE__ */ (() => {
  const $closure = (firstFunction) => {
    return (secondFunction) => {
      return (value) => {
        if (value.tag === "Pair") {
          const { _1: field0, _2: field1 } = value;
          return {
            tag: "Pair",
            _1: firstFunction(field0),
            _2: secondFunction(field1)
          };
        }
        throw new Error("Pattern match failure");
      };
    };
  };
  return { bimap: $closure };
})();
