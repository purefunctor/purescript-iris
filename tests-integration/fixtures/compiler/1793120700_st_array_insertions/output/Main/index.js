import * as Data_Array_ST from "../Data.Array.ST/index.js";
export function inserted(more) {
  const $function = Data_Array_ST.run;
  const $effect = () => {
    const array = [];
    array.push(2 | 0);
    array.push(3 | 0, 4 | 0);
    array.push(...more, 9 | 0);
    array.unshift(1 | 0);
    array.unshift(...more);
    return array;
  };
  return $function($effect);
}
export const lengths = /* @__PURE__ */ (() => {
  const array = [];
  const pushed = array.push(1 | 0);
  const pushedAll = array.push(2 | 0, 3 | 0);
  const unshifted = array.unshift(0 | 0);
  const unshiftedAll = array.unshift(-2 | 0, -1 | 0);
  const $value = [
    pushed,
    pushedAll,
    unshifted,
    unshiftedAll
  ];
  return $value;
})();
