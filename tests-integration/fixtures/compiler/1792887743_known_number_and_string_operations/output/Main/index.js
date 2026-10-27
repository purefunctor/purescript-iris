import * as Data_EuclideanRing from "../Data.EuclideanRing/index.js";
import * as Data_Semigroup from "../Data.Semigroup/index.js";
import * as $foreign from "./foreign.js";
export function numberAdd(left) {
  return (right) => {
    return left + right;
  };
}
export function numberSubtract(left) {
  return (right) => {
    return left - right;
  };
}
export function numberMultiply(left) {
  return (right) => {
    return left * right;
  };
}
export function numberDivide(left) {
  return (right) => {
    return left / right;
  };
}
export function numberSubtractGrouped(first) {
  return (second) => {
    return (third) => {
      return first - (second - third);
    };
  };
}
export function numberAddGrouped(first) {
  return (second) => {
    return (third) => {
      return first + (second + third);
    };
  };
}
export function stringAppend(left) {
  return (right) => {
    return left + right;
  };
}
export function stringAppendGrouped(first) {
  return (second) => {
    return (third) => {
      return first + (second + third);
    };
  };
}
export function numberAddOrder($boolean) {
  return observe("left")(20) + observe("right")(22);
}
export function integerDivide(left) {
  return (right) => {
    return /* @__PURE__ */ Data_EuclideanRing.div(Data_EuclideanRing.euclideanRingInt)(left)(right);
  };
}
export function arrayAppend(left) {
  return (right) => {
    return [...left, ...right];
  };
}
export function genericAppend(semigroupValueDict) {
  return (left) => (right) => /* @__PURE__ */ Data_Semigroup.append(semigroupValueDict)(left)(right);
}
export const observe = $foreign["observe"];
export const readTrace = $foreign["readTrace"];
export const partiallyAppliedDivide = /* @__PURE__ */ Data_EuclideanRing.div(Data_EuclideanRing.euclideanRingNumber)(1);
export const partiallyAppliedAppend = /* @__PURE__ */ Data_Semigroup.append(Data_Semigroup.semigroupString)("prefix");
