import * as Data_Functor_Contravariant from "../Data.Functor.Contravariant/index.js";
export const Predicate = ($value0) => ({
  tag: "Predicate",
  _1: $value0
});
export const TripleNegative = ($value0) => ({
  tag: "TripleNegative",
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
export const contravariantTripleNegative = /* @__PURE__ */ (() => {
  return { cmap: ($function) => {
    return (value) => {
      if (value.tag === "TripleNegative") {
        const { _1: field0 } = value;
        return {
          tag: "TripleNegative",
          _1: /* @__PURE__ */ contravariantPredicateDictCmap((element) => /* @__PURE__ */ contravariantPredicateDictCmap((element$1) => /* @__PURE__ */ contravariantPredicateDictCmap((element$2) => $function(element$2))(element$1))(element))(field0)
        };
      }
      throw new Error("Pattern match failure");
    };
  } };
})();
