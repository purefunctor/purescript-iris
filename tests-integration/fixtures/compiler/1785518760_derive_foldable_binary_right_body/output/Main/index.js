import * as Data_Foldable from "../Data.Foldable/index.js";
export const RightNested = ($value0) => ({
  tag: "RightNested",
  _1: $value0
});
export function foldableRightNestedType(foldablePIntDict) {
  return {
    foldr: ($function) => {
      return (accumulator) => {
        return (value) => {
          if (value.tag === "RightNested") {
            const { _1: field0 } = value;
            return /* @__PURE__ */ Data_Foldable.foldr(foldablePIntDict)((element) => (accumulator$1) => $function(element)(accumulator$1))(accumulator)(field0);
          }
          throw new Error("Pattern match failure");
        };
      };
    },
    foldl: ($function$1) => {
      return (accumulator$2) => {
        return (value$1) => {
          if (value$1.tag === "RightNested") {
            const { _1: field0$1 } = value$1;
            return /* @__PURE__ */ Data_Foldable.foldl(foldablePIntDict)((accumulator$3) => (element$1) => $function$1(accumulator$3)(element$1))(accumulator$2)(field0$1);
          }
          throw new Error("Pattern match failure");
        };
      };
    },
    foldMap: (monoidMDict) => {
      return ($function$2) => {
        return (value$2) => {
          if (value$2.tag === "RightNested") {
            const { _1: field0$2 } = value$2;
            return /* @__PURE__ */ Data_Foldable.foldMap(foldablePIntDict)(monoidMDict)((element$2) => $function$2(element$2))(field0$2);
          }
          throw new Error("Pattern match failure");
        };
      };
    }
  };
}
