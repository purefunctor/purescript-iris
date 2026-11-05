export const Fn = ($value0) => ({
  tag: "Fn",
  _1: $value0
});
export const ConstR = ($value0) => ({
  tag: "ConstR",
  _1: $value0
});
export const GoLeft = ($value0) => ({
  tag: "GoLeft",
  _1: $value0
});
export const GoRight = ($value0) => ({
  tag: "GoRight",
  _1: $value0
});
export const profunctorFn = /* @__PURE__ */ (() => {
  return { dimap: (firstFunction) => {
    return (secondFunction) => {
      return (value) => {
        if (value.tag === "Fn") {
          const { _1: field0 } = value;
          return {
            tag: "Fn",
            _1: (argument) => secondFunction(field0(firstFunction(argument)))
          };
        }
        throw new Error("Pattern match failure");
      };
    };
  } };
})();
export const profunctorConstRType = /* @__PURE__ */ (() => {
  return { dimap: (firstFunction) => {
    return (secondFunction) => {
      return (value) => {
        if (value.tag === "ConstR") {
          const { _1: field0 } = value;
          return {
            tag: "ConstR",
            _1: (argument) => field0(firstFunction(argument))
          };
        }
        throw new Error("Pattern match failure");
      };
    };
  } };
})();
export const profunctorChoice = /* @__PURE__ */ (() => {
  return { dimap: (firstFunction) => {
    return (secondFunction) => {
      return (value) => {
        if (value.tag === "GoLeft") {
          const { _1: field0 } = value;
          return {
            tag: "GoLeft",
            _1: (argument) => field0(firstFunction(argument))
          };
        }
        if (value.tag === "GoRight") {
          const { _1: field0$1 } = value;
          return {
            tag: "GoRight",
            _1: secondFunction(field0$1)
          };
        }
        throw new Error("Pattern match failure");
      };
    };
  } };
})();
