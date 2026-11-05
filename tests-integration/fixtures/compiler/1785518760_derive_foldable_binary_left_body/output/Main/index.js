import * as Data_Bifoldable from "../Data.Bifoldable/index.js";
import * as Data_Monoid from "../Data.Monoid/index.js";
export const LeftDuplicate = ($value0) => ({
  tag: "LeftDuplicate",
  _1: $value0
});
export function foldableLeftDuplicateType(bifoldablePDict) {
  return {
    foldr: ($function) => {
      return (accumulator) => {
        return (value) => {
          if (value.tag === "LeftDuplicate") {
            const { _1: field0 } = value;
            return /* @__PURE__ */ Data_Bifoldable.bifoldr(bifoldablePDict)((element) => (accumulator$1) => $function(element)(accumulator$1))((ignored) => (accumulator$2) => accumulator$2)(accumulator)(field0);
          }
          throw new Error("Pattern match failure");
        };
      };
    },
    foldl: ($function$1) => {
      return (accumulator$3) => {
        return (value$1) => {
          if (value$1.tag === "LeftDuplicate") {
            const { _1: field0$1 } = value$1;
            return /* @__PURE__ */ Data_Bifoldable.bifoldl(bifoldablePDict)((accumulator$4) => (element$1) => $function$1(accumulator$4)(element$1))((accumulator$5) => (ignored$1) => accumulator$5)(accumulator$3)(field0$1);
          }
          throw new Error("Pattern match failure");
        };
      };
    },
    foldMap: (monoidMDict) => {
      return ($function$2) => {
        return (value$2) => {
          if (value$2.tag === "LeftDuplicate") {
            const { _1: field0$2 } = value$2;
            return /* @__PURE__ */ Data_Bifoldable.bifoldMap(bifoldablePDict)(monoidMDict)((element$2) => $function$2(element$2))((ignored$2) => /* @__PURE__ */ Data_Monoid.mempty(monoidMDict))(field0$2);
          }
          throw new Error("Pattern match failure");
        };
      };
    }
  };
}
