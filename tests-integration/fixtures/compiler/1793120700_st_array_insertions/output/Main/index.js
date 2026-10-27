import * as Data_Array_ST from "../Data.Array.ST/index.js";
export function inserted(more) {
  const $function = Data_Array_ST.run;
  const $action = Data_Array_ST.new;
  const $effect = () => {
    const array = $action();
    const $action$1 = Data_Array_ST.push(2 | 0)(array);
    const $int = $action$1();
    const $action$2 = Data_Array_ST.pushAll([3 | 0, 4 | 0])(array);
    const $int$1 = $action$2();
    const $action$3 = Data_Array_ST.pushAll([...more, 9 | 0])(array);
    const $int$2 = $action$3();
    const $action$4 = Data_Array_ST.unshift(1 | 0)(array);
    const $int$3 = $action$4();
    const $action$5 = Data_Array_ST.unshiftAll(more)(array);
    const $int$4 = $action$5();
    return array;
  };
  return $function($effect);
}
export const lengths = /* @__PURE__ */ (() => {
  const $action = Data_Array_ST.new;
  const array = $action();
  const $action$1 = Data_Array_ST.push(1 | 0)(array);
  const pushed = $action$1();
  const $action$2 = Data_Array_ST.pushAll([2 | 0, 3 | 0])(array);
  const pushedAll = $action$2();
  const $action$3 = Data_Array_ST.unshift(0 | 0)(array);
  const unshifted = $action$3();
  const $action$4 = Data_Array_ST.unshiftAll([-2 | 0, -1 | 0])(array);
  const unshiftedAll = $action$4();
  const $value = [
    pushed,
    pushedAll,
    unshifted,
    unshiftedAll
  ];
  return $value;
})();
