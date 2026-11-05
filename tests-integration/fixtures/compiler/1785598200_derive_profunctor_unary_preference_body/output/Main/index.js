import * as Data_Functor from "../Data.Functor/index.js";
export const Function = ($value0) => ({
  tag: "Function",
  _1: $value0
});
export const NestedRight = ($value0) => ({
  tag: "NestedRight",
  _1: $value0
});
export const profunctorFunction = /* @__PURE__ */ (() => {
  return { dimap: (firstFunction) => {
    return (secondFunction) => {
      return (value) => {
        if (value.tag === "Function") {
          const { _1: field0 } = value;
          return {
            tag: "Function",
            _1: (argument) => secondFunction(field0(firstFunction(argument)))
          };
        }
        throw new Error("Pattern match failure");
      };
    };
  } };
})();
export const functorFunction = /* @__PURE__ */ (() => {
  return { map: ($function) => {
    return (value) => {
      if (value.tag === "Function") {
        const { _1: field0 } = value;
        return {
          tag: "Function",
          _1: (argument) => $function(field0(argument))
        };
      }
      throw new Error("Pattern match failure");
    };
  } };
})();
export const profunctorNestedRightType = /* @__PURE__ */ (() => {
  return { dimap: (firstFunction) => {
    return (secondFunction) => {
      return (value) => {
        if (value.tag === "NestedRight") {
          const { _1: field0 } = value;
          return {
            tag: "NestedRight",
            _1: /* @__PURE__ */ Data_Functor.map(functorFunction)((element) => secondFunction(element))(field0)
          };
        }
        throw new Error("Pattern match failure");
      };
    };
  } };
})();
