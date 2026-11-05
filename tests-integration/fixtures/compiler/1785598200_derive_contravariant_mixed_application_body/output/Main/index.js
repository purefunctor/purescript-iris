import * as Data_Functor from "../Data.Functor/index.js";
import * as Data_Functor_Contravariant from "../Data.Functor.Contravariant/index.js";
export const Box = ($value0) => ({
  tag: "Box",
  _1: $value0
});
export const Predicate = ($value0) => ({
  tag: "Predicate",
  _1: $value0
});
export const BoxedInput = ($value0) => ({
  tag: "BoxedInput",
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
export const contravariantPredicate = /* @__PURE__ */ (() => {
  return { cmap: ($function) => {
    return (value) => {
      if (value.tag === "Predicate") {
        const { _1: field0 } = value;
        return {
          tag: "Predicate",
          _1: (argument) => field0($function(argument))
        };
      }
      throw new Error("Pattern match failure");
    };
  } };
})();
export const contravariantBoxedInput = /* @__PURE__ */ (() => {
  return { cmap: ($function) => {
    return (value) => {
      if (value.tag === "BoxedInput") {
        const { _1: field0 } = value;
        return {
          tag: "BoxedInput",
          _1: /* @__PURE__ */ Data_Functor_Contravariant.cmap(contravariantPredicate)((element) => /* @__PURE__ */ Data_Functor.map(functorBox)((element$1) => $function(element$1))(element))(field0)
        };
      }
      throw new Error("Pattern match failure");
    };
  } };
})();
