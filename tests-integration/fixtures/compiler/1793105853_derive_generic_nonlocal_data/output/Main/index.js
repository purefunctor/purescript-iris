import * as Data_Generic_Rep from "../Data.Generic.Rep/index.js";
import * as Lib from "../Lib/index.js";
import * as $runtime from "../runtime.js";
const $lazy_genericOpaqueNoConstructors = $runtime.binding("genericOpaqueNoConstructors", () => {
  const $closure = (value) => {
    return /* @__PURE__ */ Data_Generic_Rep.to($lazy_genericOpaqueNoConstructors())(value);
  };
  const $closure$1 = (value$1) => {
    return /* @__PURE__ */ Data_Generic_Rep.from($lazy_genericOpaqueNoConstructors())(value$1);
  };
  return {
    to: $closure,
    from: $closure$1
  };
});
export const genericWrapperConstructorArgumentInt = /* @__PURE__ */ (() => {
  const $closure = (representation) => {
    const field0 = representation;
    return {
      tag: "Wrapper",
      _1: field0
    };
  };
  const $closure$1 = (value) => {
    if (value.tag === "Wrapper") {
      const { _1: field0$1 } = value;
      return field0$1;
    }
    throw new Error("Pattern match failure");
  };
  return {
    to: $closure,
    from: $closure$1
  };
})();
export const genericOpaqueNoConstructors = $lazy_genericOpaqueNoConstructors();
