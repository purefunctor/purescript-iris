export const Constant = ($value0) => ({
  tag: "Constant",
  _1: $value0
});
export const contravariantConstantType = /* @__PURE__ */ (() => {
  return { cmap: ($function) => {
    return (value) => {
      if (value.tag === "Constant") {
        const { _1: field0 } = value;
        return {
          tag: "Constant",
          _1: field0
        };
      }
      throw new Error("Pattern match failure");
    };
  } };
})();
