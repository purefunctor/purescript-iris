import * as Data_Bifunctor from "../Data.Bifunctor/index.js";
export const RightOnly = ($value0) => ({
  tag: "RightOnly",
  _1: $value0
});
export const NestedRightOnly = ($value0) => ({
  tag: "NestedRightOnly",
  _1: $value0
});
export const bifunctorRightOnlyType = /* @__PURE__ */ (() => {
  return { bimap: (firstFunction) => {
    return (secondFunction) => {
      return (value) => {
        if (value.tag === "RightOnly") {
          const { _1: field0 } = value;
          return {
            tag: "RightOnly",
            _1: secondFunction(field0)
          };
        }
        throw new Error("Pattern match failure");
      };
    };
  } };
})();
export const bifunctorNestedRightOnlyType = /* @__PURE__ */ (() => {
  return { bimap: (firstFunction) => {
    return (secondFunction) => {
      return (value) => {
        if (value.tag === "NestedRightOnly") {
          const { _1: field0 } = value;
          return {
            tag: "NestedRightOnly",
            _1: /* @__PURE__ */ Data_Bifunctor.bimap(bifunctorRightOnlyType)((unchanged) => unchanged)((element) => secondFunction(element))(field0)
          };
        }
        throw new Error("Pattern match failure");
      };
    };
  } };
})();
