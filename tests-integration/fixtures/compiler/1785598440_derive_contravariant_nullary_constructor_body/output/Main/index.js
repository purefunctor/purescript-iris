export const None = "None";
export const Some = ($value0) => ({
  tag: "Some",
  _1: $value0
});
export const contravariantOptionalPredicate = /* @__PURE__ */ (() => {
  return { cmap: ($function) => {
    return (value) => {
      if (value === "None") {
        return "None";
      }
      if (value.tag === "Some") {
        const { _1: field0 } = value;
        return {
          tag: "Some",
          _1: (argument) => field0($function(argument))
        };
      }
      throw new Error("Pattern match failure");
    };
  } };
})();
