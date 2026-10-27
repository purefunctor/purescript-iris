import * as Data_Array_ST from "../Data.Array.ST/index.js";
export function reads(values) {
  const array = values.slice();
  const $action = Data_Array_ST.length(array);
  const length = $action();
  const $action$1 = Data_Array_ST.peek(0 | 0)(array);
  const first = $action$1();
  const $action$2 = Data_Array_ST.peek(length - (1 | 0) | 0)(array);
  const last = $action$2();
  const $action$3 = Data_Array_ST.peek(length)(array);
  const past = $action$3();
  const $action$4 = Data_Array_ST.peek(-1 | 0)(array);
  const negative = $action$4();
  const $value = {
    length,
    first,
    last,
    past,
    negative
  };
  return $value;
}
