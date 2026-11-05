export const PredicateWithFormatter = ($value0) => ($value1) => ({
  tag: "PredicateWithFormatter",
  _1: $value0,
  _2: $value1
});
export const contravariantPredicateWithFormatter = /* @__PURE__ */ (() => {
  return { cmap: ($function) => {
    return (value) => {
      if (value.tag === "PredicateWithFormatter") {
        const { _1: field0, _2: field1 } = value;
        return {
          tag: "PredicateWithFormatter",
          _1: (argument) => field0($function(argument)),
          _2: field1
        };
      }
      throw new Error("Pattern match failure");
    };
  } };
})();
