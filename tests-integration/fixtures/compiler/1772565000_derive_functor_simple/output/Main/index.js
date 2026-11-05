export const Identity = ($value0) => ({
  tag: "Identity",
  _1: $value0
});
export const Const = ($value0) => ({
  tag: "Const",
  _1: $value0
});
export const Nothing = "Nothing";
export const Just = ($value0) => ({
  tag: "Just",
  _1: $value0
});
export const functorIdentity = /* @__PURE__ */ (() => {
  return { map: ($function) => {
    return (value) => {
      if (value.tag === "Identity") {
        const { _1: field0 } = value;
        return {
          tag: "Identity",
          _1: $function(field0)
        };
      }
      throw new Error("Pattern match failure");
    };
  } };
})();
export const functorConstType = /* @__PURE__ */ (() => {
  return { map: ($function) => {
    return (value) => {
      if (value.tag === "Const") {
        const { _1: field0 } = value;
        return {
          tag: "Const",
          _1: field0
        };
      }
      throw new Error("Pattern match failure");
    };
  } };
})();
export const functorMaybe = /* @__PURE__ */ (() => {
  return { map: ($function) => {
    return (value) => {
      if (value === "Nothing") {
        return "Nothing";
      }
      if (value.tag === "Just") {
        const { _1: field0 } = value;
        return {
          tag: "Just",
          _1: $function(field0)
        };
      }
      throw new Error("Pattern match failure");
    };
  } };
})();
