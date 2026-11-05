import * as Data_Profunctor from "../Data.Profunctor/index.js";
export const Nested = ($value0) => ({
  tag: "Nested",
  _1: $value0
});
export function profunctorNestedTypeType(profunctorPDict) {
  return { dimap: (firstFunction) => {
    return (secondFunction) => {
      return (value) => {
        if (value.tag === "Nested") {
          const { _1: field0 } = value;
          return {
            tag: "Nested",
            _1: /* @__PURE__ */ Data_Profunctor.dimap(profunctorPDict)((element) => firstFunction(element))((element$1) => secondFunction(element$1))(field0)
          };
        }
        throw new Error("Pattern match failure");
      };
    };
  } };
}
