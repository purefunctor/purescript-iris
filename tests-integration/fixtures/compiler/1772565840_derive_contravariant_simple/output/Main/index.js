export const Predicate = ($value0) => ({
  tag: "Predicate",
  _1: $value0
});
export const Comparison = ($value0) => ({
  tag: "Comparison",
  _1: $value0
});
export const Op = ($value0) => ({
  tag: "Op",
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
export const contravariantComparison = /* @__PURE__ */ (() => {
  return { cmap: ($function) => {
    return (value) => {
      if (value.tag === "Comparison") {
        const { _1: field0 } = value;
        return {
          tag: "Comparison",
          _1: (argument) => (argument1) => field0($function(argument))($function(argument1))
        };
      }
      throw new Error("Pattern match failure");
    };
  } };
})();
export const contravariantOp = /* @__PURE__ */ (() => {
  return { cmap: ($function) => {
    return (value) => {
      if (value.tag === "Op") {
        const { _1: field0 } = value;
        return {
          tag: "Op",
          _1: (argument) => field0($function(argument))
        };
      }
      throw new Error("Pattern match failure");
    };
  } };
})();
