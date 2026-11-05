export const Left = ($value0) => ({
  tag: "Left",
  _1: $value0
});
export const Right = ($value0) => ({
  tag: "Right",
  _1: $value0
});
export const bifoldableEither = /* @__PURE__ */ (() => {
  return {
    bifoldr: (firstFunction) => {
      return (secondFunction) => {
        return (accumulator) => {
          return (value) => {
            if (value.tag === "Left") {
              const { _1: field0 } = value;
              return firstFunction(field0)(accumulator);
            }
            if (value.tag === "Right") {
              const { _1: field0$1 } = value;
              return secondFunction(field0$1)(accumulator);
            }
            throw new Error("Pattern match failure");
          };
        };
      };
    },
    bifoldl: (firstFunction$1) => {
      return (secondFunction$1) => {
        return (accumulator$1) => {
          return (value$1) => {
            if (value$1.tag === "Left") {
              const { _1: field0$2 } = value$1;
              return firstFunction$1(accumulator$1)(field0$2);
            }
            if (value$1.tag === "Right") {
              const { _1: field0$3 } = value$1;
              return secondFunction$1(accumulator$1)(field0$3);
            }
            throw new Error("Pattern match failure");
          };
        };
      };
    },
    bifoldMap: (monoidMDict) => {
      return (firstFunction$2) => {
        return (secondFunction$2) => {
          return (value$2) => {
            if (value$2.tag === "Left") {
              const { _1: field0$4 } = value$2;
              return firstFunction$2(field0$4);
            }
            if (value$2.tag === "Right") {
              const { _1: field0$5 } = value$2;
              return secondFunction$2(field0$5);
            }
            throw new Error("Pattern match failure");
          };
        };
      };
    }
  };
})();
