import * as Data_Foldable from "../Data.Foldable/index.js";
import * as Data_Semigroup from "../Data.Semigroup/index.js";
export const Wrap = ($value0) => ($value1) => ({
  tag: "Wrap",
  _1: $value0,
  _2: $value1
});
export function bifoldableWrapTypeType(foldableFDict) {
  return (foldableGDict) => {
    return {
      bifoldr: (firstFunction) => {
        return (secondFunction) => {
          return (accumulator) => {
            return (value) => {
              if (value.tag === "Wrap") {
                const { _1: field0, _2: field1 } = value;
                return /* @__PURE__ */ Data_Foldable.foldr(foldableFDict)((element) => (accumulator$1) => firstFunction(element)(accumulator$1))(/* @__PURE__ */ Data_Foldable.foldr(foldableGDict)((element$1) => (accumulator$2) => secondFunction(element$1)(accumulator$2))(accumulator)(field1))(field0);
              }
              throw new Error("Pattern match failure");
            };
          };
        };
      },
      bifoldl: (firstFunction$1) => {
        return (secondFunction$1) => {
          return (accumulator$3) => {
            return (value$1) => {
              if (value$1.tag === "Wrap") {
                const { _1: field0$1, _2: field1$1 } = value$1;
                return /* @__PURE__ */ Data_Foldable.foldl(foldableGDict)((accumulator$4) => (element$2) => secondFunction$1(accumulator$4)(element$2))(/* @__PURE__ */ Data_Foldable.foldl(foldableFDict)((accumulator$5) => (element$3) => firstFunction$1(accumulator$5)(element$3))(accumulator$3)(field0$1))(field1$1);
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
              if (value$2.tag === "Wrap") {
                const { _1: field0$2, _2: field1$2 } = value$2;
                return /* @__PURE__ */ Data_Semigroup.append(/* @__PURE__ */ monoidMDict.Semigroup0())(/* @__PURE__ */ Data_Foldable.foldMap(foldableFDict)(monoidMDict)((element$4) => firstFunction$2(element$4))(field0$2))(/* @__PURE__ */ Data_Foldable.foldMap(foldableGDict)(monoidMDict)((element$5) => secondFunction$2(element$5))(field1$2));
              }
              throw new Error("Pattern match failure");
            };
          };
        };
      }
    };
  };
}
