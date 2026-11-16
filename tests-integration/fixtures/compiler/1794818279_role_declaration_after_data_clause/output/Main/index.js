export const Box = ($value0) => ({
  tag: "Box",
  _1: $value0
});
export const functorBox = /* @__PURE__ */ (() => {
  return { map: ($function) => {
    return (value) => {
      if (value.tag === "Box") {
        const { _1: field0 } = value;
        return {
          tag: "Box",
          _1: $function(field0)
        };
      }
      throw new Error("Pattern match failure");
    };
  } };
})();
