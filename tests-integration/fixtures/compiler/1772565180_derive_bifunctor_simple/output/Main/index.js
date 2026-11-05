export const Left = ($value0) => ({
  tag: "Left",
  _1: $value0
});
export const Right = ($value0) => ({
  tag: "Right",
  _1: $value0
});
export const Pair = ($value0) => ($value1) => ({
  tag: "Pair",
  _1: $value0,
  _2: $value1
});
export const Const2 = ($value0) => ({
  tag: "Const2",
  _1: $value0
});
export const bifunctorEither = /* @__PURE__ */ (() => {
  return { bimap: (firstFunction) => {
    return (secondFunction) => {
      return (value) => {
        if (value.tag === "Left") {
          const { _1: field0 } = value;
          return {
            tag: "Left",
            _1: firstFunction(field0)
          };
        }
        if (value.tag === "Right") {
          const { _1: field0$1 } = value;
          return {
            tag: "Right",
            _1: secondFunction(field0$1)
          };
        }
        throw new Error("Pattern match failure");
      };
    };
  } };
})();
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
export const bifunctorConst2TypeType = /* @__PURE__ */ (() => {
  return { bimap: (firstFunction) => {
    return (secondFunction) => {
      return (value) => {
        if (value.tag === "Const2") {
          const { _1: field0 } = value;
          return {
            tag: "Const2",
            _1: field0
          };
        }
        throw new Error("Pattern match failure");
      };
    };
  } };
})();
