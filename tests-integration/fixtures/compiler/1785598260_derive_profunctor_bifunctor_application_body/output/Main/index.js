import * as Data_Bifunctor from "../Data.Bifunctor/index.js";
export const Pair = ($value0) => ($value1) => ({
  tag: "Pair",
  _1: $value0,
  _2: $value1
});
export const Nested = ($value0) => ({
  tag: "Nested",
  _1: $value0
});
export const bifunctorPair = /* @__PURE__ */ (() => {
  return { bimap: (firstFunction) => {
    return (secondFunction) => {
      return (value) => {
        if (value.tag === "Pair") {
          const { _1: field0, _2: field1 } = value;
          return {
            tag: "Pair",
            _1: firstFunction(field0),
            _2: secondFunction(field1)
          };
        }
        throw new Error("Pattern match failure");
      };
    };
  } };
})();
export const profunctorNested = /* @__PURE__ */ (() => {
  return { dimap: (firstFunction) => {
    return (secondFunction) => {
      return (value) => {
        if (value.tag === "Nested") {
          const { _1: field0 } = value;
          return {
            tag: "Nested",
            _1: /* @__PURE__ */ Data_Bifunctor.bimap(bifunctorPair)((element) => (argument) => element(firstFunction(argument)))((element$1) => secondFunction(element$1))(field0)
          };
        }
        throw new Error("Pattern match failure");
      };
    };
  } };
})();
