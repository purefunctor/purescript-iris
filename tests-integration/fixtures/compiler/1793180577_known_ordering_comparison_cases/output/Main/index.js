import * as Data_Ord from "../Data.Ord/index.js";
import * as Data_Ordering from "../Data.Ordering/index.js";
export function compareIntegers(left) {
  return (right) => {
    const $scrutinee = /* @__PURE__ */ ordIntDictCompare(left)(right);
    if ($scrutinee === "LT") {
      return "less";
    }
    if ($scrutinee === "EQ") {
      return "equal";
    }
    if ($scrutinee === "GT") {
      return "greater";
    }
    throw new Error("Pattern match failure");
  };
}
export function compareStrings(left) {
  return (right) => {
    const $scrutinee = /* @__PURE__ */ Data_Ord.compare(Data_Ord.ordString)(left)(right);
    if ($scrutinee === "EQ") {
      return "equal";
    }
    if ($scrutinee === "GT") {
      return "greater";
    }
    if ($scrutinee === "LT") {
      return "less";
    }
    throw new Error("Pattern match failure");
  };
}
export function compareNumbers(left) {
  return (right) => {
    const $scrutinee = /* @__PURE__ */ ordNumberDictCompare(left)(right);
    if ($scrutinee === "GT") {
      return "greater";
    }
    if ($scrutinee === "EQ") {
      return "equal";
    }
    return "less";
  };
}
export function numberIsGreater(left) {
  return (right) => {
    const $scrutinee = /* @__PURE__ */ ordNumberDictCompare(left)(right);
    if ($scrutinee === "GT") {
      return true;
    }
    return false;
  };
}
export function computedIsLess(left) {
  return (right) => {
    const $scrutinee = /* @__PURE__ */ ordIntDictCompare(left + (1 | 0) | 0)(right * (2 | 0) | 0);
    if ($scrutinee === "LT") {
      return true;
    }
    return false;
  };
}
export function arrayIsLess(left) {
  return (right) => {
    const $scrutinee = /* @__PURE__ */ Data_Ord.compare(/* @__PURE__ */ Data_Ord.ordArray(Data_Ord.ordInt))(left)(right);
    if ($scrutinee === "LT") {
      return true;
    }
    return false;
  };
}
const ordIntDictCompare = /* @__PURE__ */ Data_Ord.compare(Data_Ord.ordInt);
const ordNumberDictCompare = /* @__PURE__ */ Data_Ord.compare(Data_Ord.ordNumber);
