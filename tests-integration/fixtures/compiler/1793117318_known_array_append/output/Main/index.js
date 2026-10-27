import * as Data_Semigroup from "../Data.Semigroup/index.js";
export function arrayAppend(left) {
  return (right) => {
    return [...left, ...right];
  };
}
export function arrayAppendChain(first) {
  return (second) => {
    return (third) => {
      return [
        ...first,
        ...second,
        ...third
      ];
    };
  };
}
export function arrayAppendGrouped(first) {
  return (second) => {
    return (third) => {
      return (fourth) => {
        return [
          ...first,
          ...second,
          ...third,
          ...fourth
        ];
      };
    };
  };
}
export function arrayAppendLiterals(middle) {
  return [
    1 | 0,
    2 | 0,
    ...middle,
    3 | 0
  ];
}
export const literalAppend = [
  1 | 0,
  2 | 0,
  3 | 0
];
export const partiallyAppliedAppend = /* @__PURE__ */ Data_Semigroup.append(Data_Semigroup.semigroupArray)([0 | 0]);
