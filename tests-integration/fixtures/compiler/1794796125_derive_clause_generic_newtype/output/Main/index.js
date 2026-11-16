export const North = "North";
export const South = "South";
export const Pair = ($value0) => ($value1) => ({
  tag: "Pair",
  _1: $value0,
  _2: $value1
});
export const genericDirectionSumConstructorNoArgumentsConstructorNoArguments = /* @__PURE__ */ (() => {
  return {
    to: (representation) => {
      if (representation.tag === "Inl" && representation._1 === "NoArguments") {
        return "North";
      }
      if (representation.tag === "Inr" && representation._1 === "NoArguments") {
        return "South";
      }
      throw new Error("Pattern match failure");
    },
    from: (value) => {
      if (value === "North") {
        return {
          tag: "Inl",
          _1: "NoArguments"
        };
      }
      if (value === "South") {
        return {
          tag: "Inr",
          _1: "NoArguments"
        };
      }
      throw new Error("Pattern match failure");
    }
  };
})();
export const genericPairConstructorProductArgumentArgument = /* @__PURE__ */ (() => {
  return {
    to: (representation) => {
      if (representation.tag === "Product") {
        const { _1: field0, _2: field1 } = representation;
        return {
          tag: "Pair",
          _1: field0,
          _2: field1
        };
      }
      throw new Error("Pattern match failure");
    },
    from: (value) => {
      if (value.tag === "Pair") {
        const { _1: field0$1, _2: field1$1 } = value;
        return {
          tag: "Product",
          _1: field0$1,
          _2: field1$1
        };
      }
      throw new Error("Pattern match failure");
    }
  };
})();
export const genericWrapperConstructorArgument = /* @__PURE__ */ (() => {
  return {
    to: (representation) => {
      const field0 = representation;
      return field0;
    },
    from: (value) => {
      const field0$1 = value;
      return field0$1;
    }
  };
})();
export const newtypeWrapper = { Coercible0: () => ({}) };
