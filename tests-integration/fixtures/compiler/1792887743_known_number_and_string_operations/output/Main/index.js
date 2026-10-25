import * as Data_EuclideanRing from "../Data.EuclideanRing/index.js";
import * as Data_Ring from "../Data.Ring/index.js";
import * as Data_Semigroup from "../Data.Semigroup/index.js";
import * as Data_Semiring from "../Data.Semiring/index.js";
import * as $foreign from "./foreign.js";
export function numberAdd(left) {
  return (right) => {
    return /* @__PURE__ */ semiringNumberDictAdd(left)(right);
  };
}
export function numberSubtract(left) {
  return (right) => {
    return /* @__PURE__ */ ringNumberDictSub(left)(right);
  };
}
export function numberMultiply(left) {
  return (right) => {
    return /* @__PURE__ */ Data_Semiring.mul(Data_Semiring.semiringNumber)(left)(right);
  };
}
export function numberDivide(left) {
  return (right) => {
    return /* @__PURE__ */ euclideanRingNumberDictDiv(left)(right);
  };
}
export function numberSubtractGrouped(first) {
  return (second) => {
    return (third) => {
      return /* @__PURE__ */ ringNumberDictSub(first)(/* @__PURE__ */ ringNumberDictSub(second)(third));
    };
  };
}
export function numberAddGrouped(first) {
  return (second) => {
    return (third) => {
      return /* @__PURE__ */ semiringNumberDictAdd(first)(/* @__PURE__ */ semiringNumberDictAdd(second)(third));
    };
  };
}
export function stringAppend(left) {
  return (right) => {
    return /* @__PURE__ */ semigroupStringDictAppend(left)(right);
  };
}
export function stringAppendGrouped(first) {
  return (second) => {
    return (third) => {
      return /* @__PURE__ */ semigroupStringDictAppend(first)(/* @__PURE__ */ semigroupStringDictAppend(second)(third));
    };
  };
}
export function numberAddOrder($boolean) {
  return /* @__PURE__ */ semiringNumberDictAdd(observe("left")(20))(observe("right")(22));
}
export function integerDivide(left) {
  return (right) => {
    return /* @__PURE__ */ Data_EuclideanRing.div(Data_EuclideanRing.euclideanRingInt)(left)(right);
  };
}
export function arrayAppend(left) {
  return (right) => {
    return /* @__PURE__ */ Data_Semigroup.append(Data_Semigroup.semigroupArray)(left)(right);
  };
}
export function genericAppend(semigroupValueDict) {
  return (left) => (right) => /* @__PURE__ */ Data_Semigroup.append(semigroupValueDict)(left)(right);
}
export const observe = $foreign["observe"];
export const readTrace = $foreign["readTrace"];
const semiringNumberDictAdd = /* @__PURE__ */ Data_Semiring.add(Data_Semiring.semiringNumber);
const ringNumberDictSub = /* @__PURE__ */ Data_Ring.sub(Data_Ring.ringNumber);
const euclideanRingNumberDictDiv = /* @__PURE__ */ Data_EuclideanRing.div(Data_EuclideanRing.euclideanRingNumber);
const semigroupStringDictAppend = /* @__PURE__ */ Data_Semigroup.append(Data_Semigroup.semigroupString);
export const partiallyAppliedDivide = /* @__PURE__ */ euclideanRingNumberDictDiv(1);
export const partiallyAppliedAppend = /* @__PURE__ */ semigroupStringDictAppend("prefix");
