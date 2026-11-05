import * as Data_Functor_Contravariant from "../Data.Functor.Contravariant/index.js";
export const Nested = ($value0) => ({
  tag: "Nested",
  _1: $value0
});
export function contravariantNestedType(contravariantFDict) {
  return { cmap: ($function) => {
    return (value) => {
      if (value.tag === "Nested") {
        const { _1: field0 } = value;
        return {
          tag: "Nested",
          _1: /* @__PURE__ */ Data_Functor_Contravariant.cmap(contravariantFDict)((element) => $function(element))(field0)
        };
      }
      throw new Error("Pattern match failure");
    };
  } };
}
