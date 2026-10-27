import * as Data_Array_ST from "../Data.Array.ST/index.js";
export function removals(values) {
  const array = values.slice();
  const $action = Data_Array_ST.pop(array);
  const last = $action();
  const $action$1 = Data_Array_ST.shift(array);
  const first = $action$1();
  const empty = [];
  const $action$2 = Data_Array_ST.pop(empty);
  const emptyLast = $action$2();
  const $action$3 = Data_Array_ST.shift(empty);
  const emptyFirst = $action$3();
  const final = array.slice();
  const $value = {
    last,
    first,
    emptyLast,
    emptyFirst,
    final
  };
  return $value;
}
