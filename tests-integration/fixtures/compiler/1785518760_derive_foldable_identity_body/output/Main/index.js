export const Identity = ($value0) => ({
  tag: "Identity",
  _1: $value0
});
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
