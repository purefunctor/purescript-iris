import * as Data_Functor_Contravariant from "../Data.Functor.Contravariant/index.js";
export const Predicate = ($value0) => ({
  tag: "Predicate",
  _1: $value0
});
export const DoubleNegative = ($value0) => ({
  tag: "DoubleNegative",
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
const contravariantPredicateDictCmap = /* @__PURE__ */ Data_Functor_Contravariant.cmap(contravariantPredicate);
export const functorDoubleNegative = /* @__PURE__ */ (() => {
  return { map: ($function) => {
    return (value) => {
      if (value.tag === "DoubleNegative") {
        const { _1: field0 } = value;
        return {
          tag: "DoubleNegative",
          _1: /* @__PURE__ */ contravariantPredicateDictCmap((element) => /* @__PURE__ */ contravariantPredicateDictCmap((element$1) => $function(element$1))(element))(field0)
        };
      }
      throw new Error("Pattern match failure");
    };
  } };
})();
