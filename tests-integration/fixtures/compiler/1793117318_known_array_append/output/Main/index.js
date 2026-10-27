import * as Data_Semigroup from "../Data.Semigroup/index.js";
export function arrayAppend(left) {
  return (right) => {
    return /* @__PURE__ */ semigroupArrayIntDictAppend(left)(right);
  };
}
export function arrayAppendChain(first) {
  return (second) => {
    return (third) => {
      return /* @__PURE__ */ semigroupArrayIntDictAppend(first)(/* @__PURE__ */ semigroupArrayIntDictAppend(second)(third));
    };
  };
}
export function arrayAppendGrouped(first) {
  return (second) => {
    return (third) => {
      return (fourth) => {
        return /* @__PURE__ */ semigroupArrayIntDictAppend(/* @__PURE__ */ semigroupArrayIntDictAppend(first)(second))(/* @__PURE__ */ semigroupArrayIntDictAppend(third)(fourth));
      };
    };
  };
}
export function arrayAppendLiterals(middle) {
  return /* @__PURE__ */ semigroupArrayIntDictAppend([1 | 0, 2 | 0])(/* @__PURE__ */ semigroupArrayIntDictAppend(middle)(/* @__PURE__ */ semigroupArrayIntDictAppend([3 | 0])([])));
}
const semigroupArrayIntDictAppend = /* @__PURE__ */ Data_Semigroup.append(Data_Semigroup.semigroupArray);
export const literalAppend = /* @__PURE__ */ semigroupArrayIntDictAppend([1 | 0])([2 | 0, 3 | 0]);
export const partiallyAppliedAppend = /* @__PURE__ */ semigroupArrayIntDictAppend([0 | 0]);
