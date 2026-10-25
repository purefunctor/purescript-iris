import * as Data_Eq from "../Data.Eq/index.js";
export function compareArrays(left) {
  return (right) => {
    if (/* @__PURE__ */ eqArrayIntDictEq(left)(right)) {
      return /* @__PURE__ */ eqArrayIntDictEq(right)(left);
    } else {
      return false;
    }
  };
}
const eqArrayIntDict = /* @__PURE__ */ Data_Eq.eqArray(Data_Eq.eqInt);
const eqArrayIntDictEq = /* @__PURE__ */ Data_Eq.eq(eqArrayIntDict);
export const initialized = compareArrays([1 | 0])([1 | 0]);
