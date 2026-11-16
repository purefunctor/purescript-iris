import * as Data_Generic_Rep from "../Data.Generic.Rep/index.js";
import * as Data_Newtype from "../Data.Newtype/index.js";
export const Phantom = ($value0) => ({
  tag: "Phantom",
  _1: $value0
});
export function roundtrip(value) {
  return /* @__PURE__ */ Data_Generic_Rep.to(genericPhantomConstructorArgumentInt)(/* @__PURE__ */ Data_Generic_Rep.from(genericPhantomConstructorArgumentInt)(value));
}
export const genericPhantomConstructorArgumentInt = /* @__PURE__ */ (() => {
  return {
    to: (representation) => {
      const field0 = representation;
      return {
        tag: "Phantom",
        _1: field0
      };
    },
    from: (value) => {
      if (value.tag === "Phantom") {
        const { _1: field0$1 } = value;
        return field0$1;
      }
      throw new Error("Pattern match failure");
    }
  };
})();
export const newtypeTagged = { Coercible0: () => ({}) };
export const genericTaggedConstructorArgument = /* @__PURE__ */ (() => {
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
export const result = /* @__PURE__ */ Data_Newtype.unwrap(newtypeTagged)(/* @__PURE__ */ Data_Newtype.wrap(newtypeTagged)(42 | 0));
export const observed = (() => {
  const $scrutinee = roundtrip({
    tag: "Phantom",
    _1: 37 | 0
  });
  if ($scrutinee.tag === "Phantom") {
    const { _1: value } = $scrutinee;
    return value;
  }
  throw new Error("Pattern match failure");
})();
