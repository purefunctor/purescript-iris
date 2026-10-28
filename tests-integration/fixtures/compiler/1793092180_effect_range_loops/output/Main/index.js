import * as $foreign from "./foreign.js";
export function sumRange(start) {
  return (end) => {
    return () => {
      let total = 0 | 0;
      const $start = traced("start")(start);
      const $end = traced("end")(end);
      for (let index = $start; index < $end; index++) {
        total = total + index | 0;
      }
      return total;
    };
  };
}
export function addTo(total) {
  return (index) => {
    return () => {
      total.value = total.value + index | 0;
    };
  };
}
export function sumWithFunction(end) {
  return () => {
    const total = { value: 0 | 0 };
    const $body = addTo(total);
    for (let $index = 0 | 0; $index < end; $index++) {
      $body($index)();
    }
    return total.value;
  };
}
export function productRange(end) {
  let total = 1 | 0;
  for (let index = 1 | 0; index < end; index++) {
    total = total * index | 0;
  }
  return total;
}
export const traced = $foreign["traced"];
