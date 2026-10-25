import * as Data_Eq from "../Data.Eq/index.js";
import * as Data_Ord from "../Data.Ord/index.js";
import * as $foreign from "./foreign.js";
export function booleanEqual(left) {
  return (right) => {
    return /* @__PURE__ */ Data_Eq.eq(Data_Eq.eqBoolean)(left)(right);
  };
}
export function integerEqual(left) {
  return (right) => {
    return /* @__PURE__ */ Data_Eq.eq(Data_Eq.eqInt)(left)(right);
  };
}
export function numberEqual(left) {
  return (right) => {
    return /* @__PURE__ */ Data_Eq.eq(Data_Eq.eqNumber)(left)(right);
  };
}
export function charNotEqual(left) {
  return (right) => {
    return /* @__PURE__ */ Data_Eq.notEq(Data_Eq.eqChar)(left)(right);
  };
}
export function stringNotEqual(left) {
  return (right) => {
    return /* @__PURE__ */ Data_Eq.notEq(Data_Eq.eqString)(left)(right);
  };
}
export function booleanLessThan(left) {
  return (right) => {
    return /* @__PURE__ */ Data_Ord.lessThan(Data_Ord.ordBoolean)(left)(right);
  };
}
export function integerLessThanOrEqual(left) {
  return (right) => {
    return /* @__PURE__ */ Data_Ord.lessThanOrEq(Data_Ord.ordInt)(left)(right);
  };
}
export function charGreaterThan(left) {
  return (right) => {
    return /* @__PURE__ */ Data_Ord.greaterThan(Data_Ord.ordChar)(left)(right);
  };
}
export function stringGreaterThanOrEqual(left) {
  return (right) => {
    return /* @__PURE__ */ Data_Ord.greaterThanOrEq(Data_Ord.ordString)(left)(right);
  };
}
export function numberNotEqual(left) {
  return (right) => {
    return /* @__PURE__ */ Data_Eq.notEq(Data_Eq.eqNumber)(left)(right);
  };
}
export function numberLessThan(left) {
  return (right) => {
    return /* @__PURE__ */ Data_Ord.lessThan(Data_Ord.ordNumber)(left)(right);
  };
}
export function numberLessThanOrEqual(left) {
  return (right) => {
    return /* @__PURE__ */ Data_Ord.lessThanOrEq(Data_Ord.ordNumber)(left)(right);
  };
}
export function numberGreaterThan(left) {
  return (right) => {
    return /* @__PURE__ */ Data_Ord.greaterThan(Data_Ord.ordNumber)(left)(right);
  };
}
export function numberGreaterThanOrEqual(left) {
  return (right) => {
    return /* @__PURE__ */ Data_Ord.greaterThanOrEq(Data_Ord.ordNumber)(left)(right);
  };
}
export function numberNotGreaterThan(left) {
  return (right) => {
    return !/* @__PURE__ */ Data_Ord.greaterThan(Data_Ord.ordNumber)(left)(right);
  };
}
export function integerBetween(low) {
  return (high) => {
    return (value) => {
      return /* @__PURE__ */ Data_Ord.lessThanOrEq(Data_Ord.ordInt)(low)(value) && /* @__PURE__ */ Data_Ord.lessThan(Data_Ord.ordInt)(value)(high);
    };
  };
}
export function comparisonOrder($boolean) {
  return /* @__PURE__ */ Data_Ord.lessThan(Data_Ord.ordInt)(observe("left")(1 | 0))(observe("right")(2 | 0));
}
export function comparisonCaseOrder(left) {
  return (flag) => {
    const $function = Data_Ord.greaterThan(Data_Ord.ordNumber)(observe("left")(left));
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
    return /* @__PURE__ */ $function($result);
  };
}
export const observe = $foreign["observe"];
export const readTrace = $foreign["readTrace"];
