import * as Data_Array from "../Data.Array/index.js";
export function arrayLength(values) {
  return values.length;
}
export function firstValue(values) {
  return values[0 | 0];
}
export function lastValue(values) {
  return values[values.length - (1 | 0) | 0];
}
export function indexWith(partialDict) {
  return (values) => /* @__PURE__ */ Data_Array.unsafeIndex(partialDict)(values);
}
