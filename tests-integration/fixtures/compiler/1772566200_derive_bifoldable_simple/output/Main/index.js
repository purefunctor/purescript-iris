import * as Data_Monoid from "../Data.Monoid/index.js";
import * as Data_Semigroup from "../Data.Semigroup/index.js";
export const Left = ($value0) => ({
  tag: "Left",
  _1: $value0
});
export const Right = ($value0) => ({
  tag: "Right",
  _1: $value0
});
export const Pair = ($value0) => ($value1) => ({
  tag: "Pair",
  _1: $value0,
  _2: $value1
});
export const Const2 = ($value0) => ({
  tag: "Const2",
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
export const bifoldablePair = /* @__PURE__ */ (() => {
  return {
    bifoldr: (firstFunction) => {
      return (secondFunction) => {
        return (accumulator) => {
          return (value) => {
            if (value.tag === "Pair") {
              const { _1: field0, _2: field1 } = value;
              return firstFunction(field0)(secondFunction(field1)(accumulator));
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
            if (value$1.tag === "Pair") {
              const { _1: field0$1, _2: field1$1 } = value$1;
              return secondFunction$1(firstFunction$1(accumulator$1)(field0$1))(field1$1);
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
            if (value$2.tag === "Pair") {
              const { _1: field0$2, _2: field1$2 } = value$2;
              return /* @__PURE__ */ Data_Semigroup.append(/* @__PURE__ */ monoidMDict.Semigroup0())(firstFunction$2(field0$2))(secondFunction$2(field1$2));
            }
            throw new Error("Pattern match failure");
          };
        };
      };
    }
  };
})();
export const bifoldableConst2TypeType = /* @__PURE__ */ (() => {
  return {
    bifoldr: (firstFunction) => {
      return (secondFunction) => {
        return (accumulator) => {
          return (value) => {
            if (value.tag === "Const2") {
              const { _1: field0 } = value;
              return accumulator;
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
            if (value$1.tag === "Const2") {
              const { _1: field0$1 } = value$1;
              return accumulator$1;
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
            if (value$2.tag === "Const2") {
              const { _1: field0$2 } = value$2;
              return /* @__PURE__ */ Data_Monoid.mempty(monoidMDict);
            }
            throw new Error("Pattern match failure");
          };
        };
      };
    }
  };
})();
