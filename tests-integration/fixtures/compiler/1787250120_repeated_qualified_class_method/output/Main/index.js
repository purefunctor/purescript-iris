import * as Data_Eq from "../Data.Eq/index.js";
import * as $runtime from "../runtime.js";
export function compareTwice(eqADict) {
  return (left) => (right) => /* @__PURE__ */ Data_Eq.eq(eqADict)(left)(right) ? /* @__PURE__ */ Data_Eq.eq(eqADict)(right)(left) : false;
}
export function compareIntsTwice(left) {
  return (right) => {
    if (left === right) {
      return right === left;
    } else {
      return false;
    }
  };
}
export function compareArraysTwice(left) {
  return (right) => {
    if (/* @__PURE__ */ eqArrayIntDictEq(left)(right)) {
      return /* @__PURE__ */ eqArrayIntDictEq(right)(left);
    } else {
      return false;
    }
  };
}
export function compareArraysOnce(left) {
  return (right) => {
    return /* @__PURE__ */ eqArrayIntDictEq(left)(right);
  };
}
export function compareGenericArraysTwice(eqADict) {
  const $closure = (left) => {
    return (right) => {
      const eqArrayDict = /* @__PURE__ */ Data_Eq.eqArray(eqADict);
      if (/* @__PURE__ */ Data_Eq.eq(eqArrayDict)(left)(right)) {
        return /* @__PURE__ */ Data_Eq.eq(eqArrayDict)(right)(left);
      } else {
        return false;
      }
    };
  };
  return $closure;
}
export function compareNestedArraysTwice(left) {
  return (right) => {
    return (nestedLeft) => {
      return (nestedRight) => {
        if (/* @__PURE__ */ eqArrayArrayIntDictEq(nestedLeft)(nestedRight)) {
          return /* @__PURE__ */ eqArrayArrayIntDictEq(nestedRight)(nestedLeft);
        } else {
          return /* @__PURE__ */ eqArrayIntDictEq(left)(right);
        }
      };
    };
  };
}
export function distinctGivens(eqADict) {
  return (eqBDict) => {
    const $closure = (leftA) => {
      return (rightA) => {
        return (leftB) => {
          return (rightB) => {
            const eqArrayDict = /* @__PURE__ */ Data_Eq.eqArray(eqADict);
            const eqArrayDict$1 = /* @__PURE__ */ Data_Eq.eqArray(eqBDict);
            if (/* @__PURE__ */ Data_Eq.eq(eqArrayDict)(leftA)(rightA)) {
              return /* @__PURE__ */ Data_Eq.eq(eqArrayDict)(rightA)(leftA);
            } else {
              if (/* @__PURE__ */ Data_Eq.eq(eqArrayDict$1)(leftB)(rightB)) {
                return /* @__PURE__ */ Data_Eq.eq(eqArrayDict$1)(rightB)(leftB);
              } else {
                return false;
              }
            }
          };
        };
      };
    };
    return $closure;
  };
}
export function distinctSubgoals(leftInt) {
  return (rightInt) => {
    return (leftBoolean) => {
      return (rightBoolean) => {
        if (/* @__PURE__ */ eqArrayIntDictEq(leftInt)(rightInt)) {
          return /* @__PURE__ */ eqArrayIntDictEq(rightInt)(leftInt);
        } else {
          if (/* @__PURE__ */ eqArrayBooleanDictEq(leftBoolean)(rightBoolean)) {
            return /* @__PURE__ */ eqArrayBooleanDictEq(rightBoolean)(leftBoolean);
          } else {
            return false;
          }
        }
      };
    };
  };
}
export function compareArraysThrice(left) {
  return (right) => {
    if (/* @__PURE__ */ eqArrayIntDictEq(left)(right)) {
      if (/* @__PURE__ */ eqArrayIntDictEq(right)(left)) {
        return /* @__PURE__ */ eqArrayIntDictEq(left)(right);
      } else {
        return false;
      }
    } else {
      return false;
    }
  };
}
export function compareNestedArraysWhole(left) {
  return (right) => {
    if (/* @__PURE__ */ eqArrayArrayIntDictEq(left)(right)) {
      return /* @__PURE__ */ eqArrayArrayIntDictEq(right)(left);
    } else {
      return false;
    }
  };
}
export function compareSuperclassArraysTwice(orderedADict) {
  const $closure = (left) => {
    return (right) => {
      const eqArrayDict = /* @__PURE__ */ Data_Eq.eqArray(/* @__PURE__ */ orderedADict.Eq0());
      if (/* @__PURE__ */ Data_Eq.eq(eqArrayDict)(left)(right)) {
        return /* @__PURE__ */ Data_Eq.eq(eqArrayDict)(right)(left);
      } else {
        return false;
      }
    };
  };
  return $closure;
}
export function compareSuperclassTwice(orderedADict) {
  const $closure = (left) => {
    return (right) => {
      const Eq0Dict = /* @__PURE__ */ orderedADict.Eq0();
      if (/* @__PURE__ */ Data_Eq.eq(Eq0Dict)(left)(right)) {
        return /* @__PURE__ */ Data_Eq.eq(Eq0Dict)(right)(left);
      } else {
        return false;
      }
    };
  };
  return $closure;
}
export function lambdaScope(left) {
  return (right) => {
    if (/* @__PURE__ */ eqArrayIntDictEq(left)(right)) {
      return ((lambdaLeft) => (lambdaRight) => /* @__PURE__ */ eqArrayIntDictEq(lambdaLeft)(lambdaRight) ? /* @__PURE__ */ eqArrayIntDictEq(lambdaRight)(lambdaLeft) : false)(left)(right);
    } else {
      return false;
    }
  };
}
export function whereIsolation(left) {
  return (right) => {
    if (((helperLeft) => (helperRight) => /* @__PURE__ */ eqArrayIntDictEq(helperLeft)(helperRight))(left)(right)) {
      return /* @__PURE__ */ eqArrayIntDictEq(left)(right);
    } else {
      return false;
    }
  };
}
export function equationScope($boolean) {
  return ($array) => {
    return ($array$1) => {
      if ($boolean === true) {
        const left = $array;
        const right = $array$1;
        return /* @__PURE__ */ eqArrayIntDictEq(left)(right);
      }
      if ($boolean === false) {
        const left$1 = $array;
        const right$1 = $array$1;
        return /* @__PURE__ */ eqArrayIntDictEq(right$1)(left$1);
      }
      throw new Error("Pattern match failure");
    };
  };
}
export function compareRecursiveTwice(left) {
  return (right) => {
    if (/* @__PURE__ */ Data_Eq.eq($lazy_eqRecursive())(left)(right)) {
      return /* @__PURE__ */ Data_Eq.eq($lazy_eqRecursive())(right)(left);
    } else {
      return false;
    }
  };
}
const $lazy_eqRecursive = $runtime.binding("eqRecursive", () => {
  return { eq: (left) => (right) => /* @__PURE__ */ Data_Eq.eq($lazy_eqRecursive())(left)(right) };
});
const eqArrayIntDict = /* @__PURE__ */ Data_Eq.eqArray(Data_Eq.eqInt);
const eqArrayBooleanDict = /* @__PURE__ */ Data_Eq.eqArray(Data_Eq.eqBoolean);
const eqArrayIntDictEq = /* @__PURE__ */ Data_Eq.eq(eqArrayIntDict);
const eqArrayArrayIntDict = /* @__PURE__ */ Data_Eq.eqArray(eqArrayIntDict);
const eqArrayBooleanDictEq = /* @__PURE__ */ Data_Eq.eq(eqArrayBooleanDict);
const eqArrayArrayIntDictEq = /* @__PURE__ */ Data_Eq.eq(eqArrayArrayIntDict);
export const firstComparison = (1 | 0) === (2 | 0);
export const secondComparison = (3 | 0) === (4 | 0);
export const eqRecursive = $lazy_eqRecursive();
