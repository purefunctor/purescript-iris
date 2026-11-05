export const contravariantEmptyType = /* @__PURE__ */ (() => {
  return { cmap: ($function) => {
    return (value) => {
      throw new Error("Pattern match failure");
    };
  } };
})();
