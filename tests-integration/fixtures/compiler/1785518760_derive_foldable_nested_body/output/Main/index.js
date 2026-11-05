import * as Data_Foldable from "../Data.Foldable/index.js";
export const Compose = ($value0) => ({
  tag: "Compose",
  _1: $value0
});
export function foldableComposeTypeType(foldableFDict) {
  return (foldableGDict) => {
    return {
      foldr: ($function) => {
        return (accumulator) => {
          return (value) => {
            if (value.tag === "Compose") {
              const { _1: field0 } = value;
              return /* @__PURE__ */ Data_Foldable.foldr(foldableFDict)((element) => (accumulator$1) => /* @__PURE__ */ Data_Foldable.foldr(foldableGDict)((element$1) => (accumulator$2) => $function(element$1)(accumulator$2))(accumulator$1)(element))(accumulator)(field0);
            }
            throw new Error("Pattern match failure");
          };
        };
      },
      foldl: ($function$1) => {
        return (accumulator$3) => {
          return (value$1) => {
            if (value$1.tag === "Compose") {
              const { _1: field0$1 } = value$1;
              return /* @__PURE__ */ Data_Foldable.foldl(foldableFDict)((accumulator$4) => (element$2) => /* @__PURE__ */ Data_Foldable.foldl(foldableGDict)((accumulator$5) => (element$3) => $function$1(accumulator$5)(element$3))(accumulator$4)(element$2))(accumulator$3)(field0$1);
            }
            throw new Error("Pattern match failure");
          };
        };
      },
      foldMap: (monoidMDict) => {
        return ($function$2) => {
          return (value$2) => {
            if (value$2.tag === "Compose") {
              const { _1: field0$2 } = value$2;
              return /* @__PURE__ */ Data_Foldable.foldMap(foldableFDict)(monoidMDict)((element$4) => /* @__PURE__ */ Data_Foldable.foldMap(foldableGDict)(monoidMDict)((element$5) => $function$2(element$5))(element$4))(field0$2);
            }
            throw new Error("Pattern match failure");
          };
        };
      }
    };
  };
}
