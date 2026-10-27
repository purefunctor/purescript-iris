import * as Data_Array_ST from "../Data.Array.ST/index.js";
export function copies(original) {
  const $action = Data_Array_ST.thaw(original);
  const array = $action();
  array.push(3 | 0);
  const $action$1 = Data_Array_ST.freeze(array);
  const frozen = $action$1();
  const $action$2 = Data_Array_ST.clone(array);
  const clone = $action$2();
  array.push(4 | 0);
  clone.push(5 | 0);
  const $action$3 = Data_Array_ST.freeze(clone);
  const cloned = $action$3();
  const $action$4 = Data_Array_ST.freeze(array);
  const final = $action$4();
  const $value = {
    original,
    frozen,
    cloned,
    final
  };
  return $value;
}
