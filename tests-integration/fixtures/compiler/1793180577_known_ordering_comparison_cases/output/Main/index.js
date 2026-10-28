import * as Data_Ord from "../Data.Ord/index.js";
import * as Data_Ordering from "../Data.Ordering/index.js";
export function compareIntegers(left) {
  return (right) => {
    if (left < right) {
      return "less";
    } else {
      if (left === right) {
        return "equal";
      } else {
        return "greater";
      }
    }
  };
}
export function compareStrings(left) {
  return (right) => {
    if (left === right) {
      return "equal";
    } else {
      if (left > right) {
        return "greater";
      } else {
        return "less";
      }
    }
  };
}
export function compareNumbers(left) {
  return (right) => {
    if (!(left <= right)) {
      return "greater";
    } else {
      if (left === right) {
        return "equal";
      } else {
        return "less";
      }
    }
  };
}
export function numberIsGreater(left) {
  return (right) => {
    if (!(left <= right)) {
      return true;
    } else {
      return false;
    }
  };
}
export function computedIsLess(left) {
  return (right) => {
    if ((left + (1 | 0) | 0) < (right * (2 | 0) | 0)) {
      return true;
    } else {
      return false;
    }
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
