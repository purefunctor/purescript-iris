import * as Data_Functor_Contravariant from "../Data.Functor.Contravariant/index.js";
import * as Data_Profunctor from "../Data.Profunctor/index.js";
export const Predicate = ($value0) => ({
  tag: "Predicate",
  _1: $value0
});
export const Function = ($value0) => ({
  tag: "Function",
  _1: $value0
});
export const Mixed = ($value0) => ({
  tag: "Mixed",
  _1: $value0
});
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
export const profunctorFunction = /* @__PURE__ */ (() => {
  return { dimap: (firstFunction) => {
    return (secondFunction) => {
      return (value) => {
        if (value.tag === "Function") {
          const { _1: field0 } = value;
          return {
            tag: "Function",
            _1: (argument) => secondFunction(field0(firstFunction(argument)))
          };
        }
        throw new Error("Pattern match failure");
      };
    };
  } };
})();
export const bifunctorMixed = /* @__PURE__ */ (() => {
  return { bimap: (firstFunction) => {
    return (secondFunction) => {
      return (value) => {
        if (value.tag === "Mixed") {
          const { _1: field0 } = value;
          return {
            tag: "Mixed",
            _1: /* @__PURE__ */ Data_Profunctor.dimap(profunctorFunction)((element) => /* @__PURE__ */ Data_Functor_Contravariant.cmap(contravariantPredicate)((element$1) => firstFunction(element$1))(element))((element$2) => secondFunction(element$2))(field0)
          };
        }
        throw new Error("Pattern match failure");
      };
    };
  } };
})();
