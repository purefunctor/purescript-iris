import * as Data_Array_ST from "../Data.Array.ST/index.js";
export function writes(values) {
  const array = values.slice();
  const $action = Data_Array_ST.poke(1 | 0)(10 | 0)(array);
  const inside = $action();
  const $action$1 = Data_Array_ST.poke(3 | 0)(20 | 0)(array);
  const past = $action$1();
  const $action$2 = Data_Array_ST.poke(-1 | 0)(30 | 0)(array);
  const negative = $action$2();
  const $action$3 = Data_Array_ST.poke(0 | 0)(40 | 0)(array);
  const $boolean = $action$3();
  const $action$4 = Data_Array_ST.modify(2 | 0)((section114) => section114 * (2 | 0) | 0)(array);
  const modified = $action$4();
  const $action$5 = Data_Array_ST.modify(3 | 0)((section130) => section130 * (2 | 0) | 0)(array);
  const unmodified = $action$5();
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
