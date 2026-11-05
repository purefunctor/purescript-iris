import * as Data_Functor from "../Data.Functor/index.js";
export const Box = ($value0) => ({
  tag: "Box",
  _1: $value0
});
export const BoxedPredicate = ($value0) => ({
  tag: "BoxedPredicate",
  _1: $value0
});
export const functorBox = /* @__PURE__ */ (() => {
  return { map: ($function) => {
    return (value) => {
      if (value.tag === "Box") {
        const { _1: field0 } = value;
        return {
          tag: "Box",
          _1: $function(field0)
        };
      }
      throw new Error("Pattern match failure");
    };
  } };
})();
export const contravariantBoxedPredicate = /* @__PURE__ */ (() => {
  return { cmap: ($function) => {
    return (value) => {
      if (value.tag === "BoxedPredicate") {
        const { _1: field0 } = value;
        return {
          tag: "BoxedPredicate",
          _1: /* @__PURE__ */ Data_Functor.map(functorBox)((element) => (argument) => element($function(argument)))(field0)
        };
      }
      throw new Error("Pattern match failure");
    };
  } };
})();
