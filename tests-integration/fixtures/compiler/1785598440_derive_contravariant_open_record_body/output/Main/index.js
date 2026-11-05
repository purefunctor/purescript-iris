export const OpenPredicate = ($value0) => ({
  tag: "OpenPredicate",
  _1: $value0
});
export const contravariantOpenPredicate = /* @__PURE__ */ (() => {
  return { cmap: ($function) => {
    return (value) => {
      if (value.tag === "OpenPredicate") {
        const { _1: field0 } = value;
        return {
          tag: "OpenPredicate",
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
