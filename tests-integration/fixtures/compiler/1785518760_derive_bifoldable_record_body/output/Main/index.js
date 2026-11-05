import * as Data_Semigroup from "../Data.Semigroup/index.js";
export const RecordPair = ($value0) => ({
  tag: "RecordPair",
  _1: $value0
});
export const bifoldableRecordPair = /* @__PURE__ */ (() => {
  return {
    bifoldr: (firstFunction) => {
      return (secondFunction) => {
        return (accumulator) => {
          return (value) => {
            if (value.tag === "RecordPair") {
              const { _1: field0 } = value;
              return firstFunction(field0.alpha)(secondFunction(field0.zeta)(accumulator));
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
            if (value$1.tag === "RecordPair") {
              const { _1: field0$1 } = value$1;
              return secondFunction$1(firstFunction$1(accumulator$1)(field0$1.alpha))(field0$1.zeta);
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
            if (value$2.tag === "RecordPair") {
              const { _1: field0$2 } = value$2;
              return /* @__PURE__ */ Data_Semigroup.append(/* @__PURE__ */ monoidMDict.Semigroup0())(firstFunction$2(field0$2.alpha))(secondFunction$2(field0$2.zeta));
            }
            throw new Error("Pattern match failure");
          };
        };
      };
    }
  };
})();
