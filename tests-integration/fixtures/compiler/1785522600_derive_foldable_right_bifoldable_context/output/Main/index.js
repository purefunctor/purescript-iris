import * as Data_Bifoldable from "../Data.Bifoldable/index.js";
import * as Data_Monoid from "../Data.Monoid/index.js";
export const RightNested = ($value0) => ({
  tag: "RightNested",
  _1: $value0
});
export function foldableRightNestedType(bifoldablePDict) {
  return {
    foldr: ($function) => {
      return (accumulator) => {
        return (value) => {
          if (value.tag === "RightNested") {
            const { _1: field0 } = value;
            return /* @__PURE__ */ Data_Bifoldable.bifoldr(bifoldablePDict)((ignored) => (accumulator$1) => accumulator$1)((element) => (accumulator$2) => $function(element)(accumulator$2))(accumulator)(field0);
          }
          throw new Error("Pattern match failure");
        };
      };
    },
    foldl: ($function$1) => {
      return (accumulator$3) => {
        return (value$1) => {
          if (value$1.tag === "RightNested") {
            const { _1: field0$1 } = value$1;
            return /* @__PURE__ */ Data_Bifoldable.bifoldl(bifoldablePDict)((accumulator$4) => (ignored$1) => accumulator$4)((accumulator$5) => (element$1) => $function$1(accumulator$5)(element$1))(accumulator$3)(field0$1);
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
            return /* @__PURE__ */ Data_Bifoldable.bifoldMap(bifoldablePDict)(monoidMDict)((ignored$2) => /* @__PURE__ */ Data_Monoid.mempty(monoidMDict))((element$2) => $function$2(element$2))(field0$2);
          }
          throw new Error("Pattern match failure");
        };
      };
    }
  };
}
