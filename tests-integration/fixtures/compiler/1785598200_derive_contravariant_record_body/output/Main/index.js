export const PredicateRecord = ($value0) => ({
  tag: "PredicateRecord",
  _1: $value0
});
export const contravariantPredicateRecord = /* @__PURE__ */ (() => {
  return { cmap: ($function) => {
    return (value) => {
      if (value.tag === "PredicateRecord") {
        const { _1: field0 } = value;
        return {
          tag: "PredicateRecord",
          _1: {
            ...field0,
            run: (argument) => field0.run($function(argument))
          }
        };
      }
      throw new Error("Pattern match failure");
    };
  } };
})();
