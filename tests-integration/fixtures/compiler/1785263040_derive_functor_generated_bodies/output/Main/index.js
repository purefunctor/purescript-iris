import * as Data_Functor from "../Data.Functor/index.js";
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
export const Wrap = ($value0) => ({
  tag: "Wrap",
  _1: $value0
});
export const Compose = ($value0) => ({
  tag: "Compose",
  _1: $value0
});
export const Reader = ($value0) => ({
  tag: "Reader",
  _1: $value0
});
export const NestedReader = ($value0) => ({
  tag: "NestedReader",
  _1: $value0
});
export const Cont = ($value0) => ({
  tag: "Cont",
  _1: $value0
});
export const Record = ($value0) => ({
  tag: "Record",
  _1: $value0
});
export const OpenRecord = ($value0) => ({
  tag: "OpenRecord",
  _1: $value0
});
export function functorWrapType(functorFDict) {
  return { map: ($function) => {
    return (value) => {
      if (value.tag === "Wrap") {
        const { _1: field0 } = value;
        return {
          tag: "Wrap",
          _1: /* @__PURE__ */ Data_Functor.map(functorFDict)((element) => $function(element))(field0)
        };
      }
      throw new Error("Pattern match failure");
    };
  } };
}
export function functorComposeTypeType(functorFDict) {
  return (functorGDict) => {
    return { map: ($function) => {
      return (value) => {
        if (value.tag === "Compose") {
          const { _1: field0 } = value;
          return {
            tag: "Compose",
            _1: /* @__PURE__ */ Data_Functor.map(functorFDict)((element) => /* @__PURE__ */ Data_Functor.map(functorGDict)((element$1) => $function(element$1))(element))(field0)
          };
        }
        throw new Error("Pattern match failure");
      };
    } };
  };
}
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
export const functorReader = /* @__PURE__ */ (() => {
  return { map: ($function) => {
    return (value) => {
      if (value.tag === "Reader") {
        const { _1: field0 } = value;
        return {
          tag: "Reader",
          _1: (argument) => $function(field0(argument))
        };
      }
      throw new Error("Pattern match failure");
    };
  } };
})();
export const functorNestedReader = /* @__PURE__ */ (() => {
  return { map: ($function) => {
    return (value) => {
      if (value.tag === "NestedReader") {
        const { _1: field0 } = value;
        return {
          tag: "NestedReader",
          _1: (argument) => (argument1) => $function(field0(argument)(argument1))
        };
      }
      throw new Error("Pattern match failure");
    };
  } };
})();
export const functorCont = /* @__PURE__ */ (() => {
  return { map: ($function) => {
    return (value) => {
      if (value.tag === "Cont") {
        const { _1: field0 } = value;
        return {
          tag: "Cont",
          _1: (argument) => field0((argument1) => argument($function(argument1)))
        };
      }
      throw new Error("Pattern match failure");
    };
  } };
})();
export const functorRecord = /* @__PURE__ */ (() => {
  return { map: ($function) => {
    return (value) => {
      if (value.tag === "Record") {
        const { _1: field0 } = value;
        return {
          tag: "Record",
          _1: {
            ...field0,
            changed: $function(field0.changed)
          }
        };
      }
      throw new Error("Pattern match failure");
    };
  } };
})();
export const functorOpenRecord = /* @__PURE__ */ (() => {
  return { map: ($function) => {
    return (value) => {
      if (value.tag === "OpenRecord") {
        const { _1: field0 } = value;
        return {
          tag: "OpenRecord",
          _1: {
            ...field0,
            changed: $function(field0.changed)
          }
        };
      }
      throw new Error("Pattern match failure");
    };
  } };
})();
