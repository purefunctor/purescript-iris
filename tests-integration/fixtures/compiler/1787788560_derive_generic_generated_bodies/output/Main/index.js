import * as Data_Generic_Rep from "../Data.Generic.Rep/index.js";
import * as $runtime from "../runtime.js";
export const Unit = "Unit";
export const Identity = ($value0) => ({
  tag: "Identity",
  _1: $value0
});
export const Pair = ($value0) => ($value1) => ({
  tag: "Pair",
  _1: $value0,
  _2: $value1
});
export const Left = ($value0) => ({
  tag: "Left",
  _1: $value0
});
export const Right = ($value0) => ({
  tag: "Right",
  _1: $value0
});
const $lazy_genericVoidNoConstructors = $runtime.binding("genericVoidNoConstructors", () => {
  return {
    to: (value) => {
      return /* @__PURE__ */ Data_Generic_Rep.to($lazy_genericVoidNoConstructors())(value);
    },
    from: (value$1) => {
      return /* @__PURE__ */ Data_Generic_Rep.from($lazy_genericVoidNoConstructors())(value$1);
    }
  };
});
export const genericVoidNoConstructors = $lazy_genericVoidNoConstructors();
export const genericUnitConstructorNoArguments = /* @__PURE__ */ (() => {
  return {
    to: (representation) => {
      if (representation === "NoArguments") {
        return "Unit";
      }
      throw new Error("Pattern match failure");
    },
    from: (value) => {
      if (value === "Unit") {
        return "NoArguments";
      }
      throw new Error("Pattern match failure");
    }
  };
})();
export const genericIdentityConstructorArgument = /* @__PURE__ */ (() => {
  return {
    to: (representation) => {
      const field0 = representation;
      return {
        tag: "Identity",
        _1: field0
      };
    },
    from: (value) => {
      if (value.tag === "Identity") {
        const { _1: field0$1 } = value;
        return field0$1;
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
export const genericEitherSumConstructorArgumentConstructorArgument = /* @__PURE__ */ (() => {
  return {
    to: (representation) => {
      if (representation.tag === "Inl") {
        const { _1: field0 } = representation;
        return {
          tag: "Left",
          _1: field0
        };
      }
      if (representation.tag === "Inr") {
        const { _1: field0$1 } = representation;
        return {
          tag: "Right",
          _1: field0$1
        };
      }
      throw new Error("Pattern match failure");
    },
    from: (value) => {
      if (value.tag === "Left") {
        const { _1: field0$2 } = value;
        return {
          tag: "Inl",
          _1: field0$2
        };
      }
      if (value.tag === "Right") {
        const { _1: field0$3 } = value;
        return {
          tag: "Inr",
          _1: field0$3
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
