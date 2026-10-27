import * as Data_Array_ST from "../Data.Array.ST/index.js";
export function shared(values) {
  const $action = Data_Array_ST.unsafeThaw(values);
  const array = $action();
  const $action$1 = Data_Array_ST.unsafeFreeze(array);
  const frozen = $action$1();
  array.push(0 | 0);
  return frozen;
}
export const literal = /* @__PURE__ */ (() => {
  const $action = Data_Array_ST.unsafeThaw([1 | 0, 2 | 0]);
  const array = $action();
  array.push(3 | 0);
  return Data_Array_ST.unsafeFreeze(array)();
})();
