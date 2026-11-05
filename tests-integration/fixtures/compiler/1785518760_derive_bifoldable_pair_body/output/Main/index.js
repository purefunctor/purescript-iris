import * as Data_Semigroup from "../Data.Semigroup/index.js";
export const Pair = ($value0) => ($value1) => ($value2) => ({
  tag: "Pair",
  _1: $value0,
  _2: $value1,
  _3: $value2
});
export const bifoldablePair = /* @__PURE__ */ (() => {
  return {
    bifoldr: (firstFunction) => {
      return (secondFunction) => {
        return (accumulator) => {
          return (value) => {
            if (value.tag === "Pair") {
              const { _1: field0, _2: field1, _3: field2 } = value;
              return firstFunction(field0)(secondFunction(field2)(accumulator));
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
              const { _1: field0$1, _2: field1$1, _3: field2$1 } = value$1;
              return secondFunction$1(firstFunction$1(accumulator$1)(field0$1))(field2$1);
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
              const { _1: field0$2, _2: field1$2, _3: field2$2 } = value$2;
              return /* @__PURE__ */ Data_Semigroup.append(/* @__PURE__ */ monoidMDict.Semigroup0())(firstFunction$2(field0$2))(secondFunction$2(field2$2));
            }
            throw new Error("Pattern match failure");
          };
        };
      };
    }
  };
})();
