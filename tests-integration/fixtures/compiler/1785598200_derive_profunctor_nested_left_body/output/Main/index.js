import * as Data_Profunctor from "../Data.Profunctor/index.js";
export const Function = ($value0) => ({
  tag: "Function",
  _1: $value0
});
export const NestedLeft = ($value0) => ({
  tag: "NestedLeft",
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
export const profunctorNestedLeftType = /* @__PURE__ */ (() => {
  return { dimap: (firstFunction) => {
    return (secondFunction) => {
      return (value) => {
        if (value.tag === "NestedLeft") {
          const { _1: field0 } = value;
          return {
            tag: "NestedLeft",
            _1: /* @__PURE__ */ Data_Profunctor.dimap(profunctorFunction)((element) => firstFunction(element))((unchanged) => unchanged)(field0)
          };
        }
        throw new Error("Pattern match failure");
      };
    };
  } };
})();
