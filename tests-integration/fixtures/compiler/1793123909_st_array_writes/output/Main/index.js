import * as Data_Array_ST from "../Data.Array.ST/index.js";
export function writes(values) {
  const array = values.slice();
  const $inBounds = (1 | 0) >= 0 && (1 | 0) < array.length;
  if ($inBounds) {
    array[1 | 0] = 10 | 0;
  }
  const inside = $inBounds;
  const $inBounds$1 = (3 | 0) >= 0 && (3 | 0) < array.length;
  if ($inBounds$1) {
    array[3 | 0] = 20 | 0;
  }
  const past = $inBounds$1;
  const $inBounds$2 = (-1 | 0) >= 0 && (-1 | 0) < array.length;
  if ($inBounds$2) {
    array[-1 | 0] = 30 | 0;
  }
  const negative = $inBounds$2;
  const $inBounds$3 = (0 | 0) >= 0 && (0 | 0) < array.length;
  if ($inBounds$3) {
    array[0 | 0] = 40 | 0;
  }
  const $action = Data_Array_ST.modify(2 | 0)((section114) => section114 * (2 | 0) | 0)(array);
  const modified = $action();
  const $action$1 = Data_Array_ST.modify(3 | 0)((section130) => section130 * (2 | 0) | 0)(array);
  const unmodified = $action$1();
  const final = array.slice();
  const $value = {
    inside,
    past,
    negative,
    modified,
    unmodified,
    final
  };
  return $value;
}
