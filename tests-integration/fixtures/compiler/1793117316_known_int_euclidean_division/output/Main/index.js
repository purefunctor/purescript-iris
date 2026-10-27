import * as Data_EuclideanRing from "../Data.EuclideanRing/index.js";
export function integerDivide(left) {
  return (right) => {
    return /* @__PURE__ */ euclideanRingIntDictDiv(left)(right);
  };
}
export function integerModulo(left) {
  return (right) => {
    return /* @__PURE__ */ euclideanRingIntDictMod(left)(right);
  };
}
export function halveAndRemainder(value) {
  return /* @__PURE__ */ euclideanRingIntDictDiv(value)(2 | 0) + /* @__PURE__ */ euclideanRingIntDictMod(value)(2 | 0) | 0;
}
const euclideanRingIntDictDiv = /* @__PURE__ */ Data_EuclideanRing.div(Data_EuclideanRing.euclideanRingInt);
const euclideanRingIntDictMod = /* @__PURE__ */ Data_EuclideanRing.mod(Data_EuclideanRing.euclideanRingInt);
export const partiallyAppliedDivide = /* @__PURE__ */ euclideanRingIntDictDiv(100 | 0);
