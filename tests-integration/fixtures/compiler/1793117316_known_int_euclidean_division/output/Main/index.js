import * as Data_EuclideanRing from "../Data.EuclideanRing/index.js";
import * as $runtime from "../runtime.js";
export function integerDivide(left) {
  return (right) => {
    return /* @__PURE__ */ $runtime.intDiv(left, right);
  };
}
export function integerModulo(left) {
  return (right) => {
    return /* @__PURE__ */ $runtime.intMod(left, right);
  };
}
export function halveAndRemainder(value) {
  return /* @__PURE__ */ $runtime.intDiv(value, 2 | 0) + /* @__PURE__ */ $runtime.intMod(value, 2 | 0) | 0;
}
export const partiallyAppliedDivide = /* @__PURE__ */ Data_EuclideanRing.div(Data_EuclideanRing.euclideanRingInt)(100 | 0);
