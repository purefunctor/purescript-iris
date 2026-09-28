import * as Data_HeytingAlgebra from "../Data.HeytingAlgebra/index.js";
import * as Data_Ring from "../Data.Ring/index.js";
import * as Data_Semiring from "../Data.Semiring/index.js";
import * as Lookalike from "../Lookalike/index.js";
import * as $foreign from "./foreign.js";
export function booleanNot(value) {
  return !value;
}
export function booleanConj(left) {
  return (right) => {
    return left && right;
  };
}
export function booleanDisj(left) {
  return (right) => {
    return left || right;
  };
}
export function genericConj(heytingAlgebraValueDict) {
  return (left) => (right) => /* @__PURE__ */ Data_HeytingAlgebra.conj(heytingAlgebraValueDict)(left)(right);
}
export function conjOrder(left) {
  return (right) => {
    return observe("left")(left) && observe("right")(right);
  };
}
export function disjOrder(left) {
  return (right) => {
    return observe("left")(left) || observe("right")(right);
  };
}
export function conjCase(left) {
  return (right) => {
    let $result;
    if (observe("left")(left)) {
      const $scrutinee = observe("right")(right);
      $case: {
        if ($scrutinee === true) {
          $result = true;
          break $case;
        }
        if ($scrutinee === false) {
          $result = false;
          break $case;
        }
        throw new Error("Pattern match failure");
      }
    } else {
      $result = false;
    }
    return $result;
  };
}
export function disjCase(left) {
  return (right) => {
    let $result;
    if (observe("left")(left)) {
      $result = true;
    } else {
      const $scrutinee = observe("right")(right);
      $case: {
        if ($scrutinee === true) {
          $result = true;
          break $case;
        }
        if ($scrutinee === false) {
          $result = false;
          break $case;
        }
        throw new Error("Pattern match failure");
      }
    }
    return $result;
  };
}
export function conjFunctionOrder(left) {
  return (right) => {
    const $function = observe("function")((value) => value);
    let $result;
    if (observe("left")(left)) {
      const $scrutinee = observe("right")(right);
      $case: {
        if ($scrutinee === true) {
          $result = true;
          break $case;
        }
        if ($scrutinee === false) {
          $result = false;
          break $case;
        }
        throw new Error("Pattern match failure");
      }
    } else {
      $result = false;
    }
    return $function($result);
  };
}
export function conjPatternFunctionOrder(left) {
  return (right) => {
    const $function = observe("function")((value) => value);
    let $result;
    if (observe("left")(left)) {
      const $closure = ($record) => {
        const value$1 = $record.value;
        return value$1;
      };
      $result = $closure({ value: observe("right")(right) });
    } else {
      $result = false;
    }
    return $function($result);
  };
}
export function integerAdd(left) {
  return (right) => {
    return left + right | 0;
  };
}
export function inlineIntegerAdd(left) {
  return (right) => {
    return left + right | 0;
  };
}
export function integerSubtract(left) {
  return (right) => {
    return left - right | 0;
  };
}
export function integerMultiply(left) {
  return (right) => {
    return left * right | 0;
  };
}
export function integerNegate(value) {
  return -value | 0;
}
export function numberNegate(value) {
  return -value;
}
export function genericNegate(ringValueDict) {
  return (value) => /* @__PURE__ */ Data_Ring.negate(ringValueDict)(value);
}
export function integerAddOrder($boolean) {
  return observe("left")(20 | 0) + observe("right")(22 | 0) | 0;
}
export function lookalikeAdd(left) {
  return (right) => {
    return /* @__PURE__ */ Lookalike.add(Lookalike.semiringInt)(left)(right);
  };
}
export function lookalikeNegate(value) {
  return Lookalike.negate(value);
}
export const observe = $foreign["observe"];
export const readTrace = $foreign["readTrace"];
export const partiallyAppliedConj = /* @__PURE__ */ Data_HeytingAlgebra.conj(Data_HeytingAlgebra.heytingAlgebraBoolean)(true);
export const integerNegateLiteral = -20 | 0;
export const inlineIntegerNegateLiteral = -20 | 0;
export const numberNegateLiteral = -20.5;
export const numberNegateZero = -0;
export const numberNegateNegativeZero = 0;
export const syntacticNegativeZero = -0;
export const partiallyAppliedNegate = /* @__PURE__ */ Data_Ring.negate(Data_Ring.ringInt);
export const partiallyAppliedAdd = /* @__PURE__ */ Data_Semiring.add(Data_Semiring.semiringInt)(1 | 0);
