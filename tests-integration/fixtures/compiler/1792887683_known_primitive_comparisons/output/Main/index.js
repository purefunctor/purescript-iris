import * as $foreign from "./foreign.js";
export function booleanEqual(left) {
  return (right) => {
    return left === right;
  };
}
export function integerEqual(left) {
  return (right) => {
    return left === right;
  };
}
export function numberEqual(left) {
  return (right) => {
    return left === right;
  };
}
export function charNotEqual(left) {
  return (right) => {
    return left !== right;
  };
}
export function stringNotEqual(left) {
  return (right) => {
    return left !== right;
  };
}
export function booleanLessThan(left) {
  return (right) => {
    return left < right;
  };
}
export function integerLessThanOrEqual(left) {
  return (right) => {
    return left <= right;
  };
}
export function charGreaterThan(left) {
  return (right) => {
    return left > right;
  };
}
export function stringGreaterThanOrEqual(left) {
  return (right) => {
    return left >= right;
  };
}
export function numberNotEqual(left) {
  return (right) => {
    return left !== right;
  };
}
export function numberLessThan(left) {
  return (right) => {
    return left < right;
  };
}
export function numberLessThanOrEqual(left) {
  return (right) => {
    return left <= right;
  };
}
export function numberGreaterThan(left) {
  return (right) => {
    return !(left <= right);
  };
}
export function numberGreaterThanOrEqual(left) {
  return (right) => {
    return !(left < right);
  };
}
export function numberNotGreaterThan(left) {
  return (right) => {
    return !!(left <= right);
  };
}
export function integerBetween(low) {
  return (high) => {
    return (value) => {
      return low <= value && value < high;
    };
  };
}
export function comparisonOrder($boolean) {
  return observe("left")(1 | 0) < observe("right")(2 | 0);
}
export function comparisonCaseOrder(left) {
  return (flag) => {
    const $left = observe("left")(left);
    let $result;
    const $scrutinee = observe("flag")(flag);
    $case: {
      if ($scrutinee === true) {
        $result = observe("right")(1);
        break $case;
      }
      if ($scrutinee === false) {
        $result = observe("right")(2);
        break $case;
      }
      throw new Error("Pattern match failure");
    }
    return !($left <= $result);
  };
}
export const observe = $foreign["observe"];
export const readTrace = $foreign["readTrace"];
