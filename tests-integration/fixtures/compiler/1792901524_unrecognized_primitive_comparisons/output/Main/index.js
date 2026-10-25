import * as Data_Eq from "../Data.Eq/index.js";
import * as Data_Ord from "../Data.Ord/index.js";
import * as Lookalike from "../Lookalike/index.js";
export function genericEqual(eqValueDict) {
  return (left) => (right) => /* @__PURE__ */ Data_Eq.eq(eqValueDict)(left)(right);
}
export function genericLessThan(ordValueDict) {
  return (left) => (right) => /* @__PURE__ */ Data_Ord.lessThan(ordValueDict)(left)(right);
}
export function unitEqual(left) {
  return (right) => {
    return /* @__PURE__ */ Data_Eq.eq(Data_Eq.eqUnit)(left)(right);
  };
}
export function wrappedEqual(left) {
  return (right) => {
    return /* @__PURE__ */ Data_Eq.eq(Lookalike.eqInt)(left)(right);
  };
}
export function wrappedGreaterThan(left) {
  return (right) => {
    return /* @__PURE__ */ Data_Ord.greaterThan(Lookalike.ordInt)(left)(right);
  };
}
export function lookalikeLessThan(left) {
  return (right) => {
    return Lookalike.lessThan(left)(right);
  };
}
export const partiallyAppliedEqual = /* @__PURE__ */ Data_Eq.eq(Data_Eq.eqInt)(1 | 0);
export const partiallyAppliedLessThan = /* @__PURE__ */ Data_Ord.lessThan(Data_Ord.ordInt)(1 | 0);
