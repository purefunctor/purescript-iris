import * as Data_Array from "../Data.Array/index.js";
import * as Partial_Unsafe from "../Partial.Unsafe/index.js";
export function arrayLength(values) {
  return values.length;
}
export function firstValue(values) {
  return Partial_Unsafe.unsafePartial((partialDict) => values[0 | 0]);
}
export function lastValue(values) {
  return Partial_Unsafe.unsafePartial((partialDict) => values[values.length - (1 | 0) | 0]);
}
export function indexWith(partialDict) {
  return (values) => /* @__PURE__ */ Data_Array.unsafeIndex(partialDict)(values);
}
