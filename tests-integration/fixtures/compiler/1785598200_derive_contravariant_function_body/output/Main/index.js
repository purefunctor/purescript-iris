export const Predicate = ($value0) => ({
  tag: "Predicate",
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
