import * as Data_Bifoldable from "../Data.Bifoldable/index.js";
export const Duplicate = ($value0) => ({
  tag: "Duplicate",
  _1: $value0
});
export function foldableDuplicateType(bifoldablePDict) {
  return {
    foldr: ($function) => {
      return (accumulator) => {
        return (value) => {
          if (value.tag === "Duplicate") {
            const { _1: field0 } = value;
            return /* @__PURE__ */ Data_Bifoldable.bifoldr(bifoldablePDict)((element) => (accumulator$1) => $function(element)(accumulator$1))((element$1) => (accumulator$2) => $function(element$1)(accumulator$2))(accumulator)(field0);
          }
          throw new Error("Pattern match failure");
        };
      };
    },
    foldl: ($function$1) => {
      return (accumulator$3) => {
        return (value$1) => {
          if (value$1.tag === "Duplicate") {
            const { _1: field0$1 } = value$1;
            return /* @__PURE__ */ Data_Bifoldable.bifoldl(bifoldablePDict)((accumulator$4) => (element$2) => $function$1(accumulator$4)(element$2))((accumulator$5) => (element$3) => $function$1(accumulator$5)(element$3))(accumulator$3)(field0$1);
          }
          throw new Error("Pattern match failure");
        };
      };
    },
    foldMap: (monoidMDict) => {
      return ($function$2) => {
        return (value$2) => {
          if (value$2.tag === "Duplicate") {
            const { _1: field0$2 } = value$2;
            return /* @__PURE__ */ Data_Bifoldable.bifoldMap(bifoldablePDict)(monoidMDict)((element$4) => $function$2(element$4))((element$5) => $function$2(element$5))(field0$2);
          }
          throw new Error("Pattern match failure");
        };
      };
    }
  };
}
