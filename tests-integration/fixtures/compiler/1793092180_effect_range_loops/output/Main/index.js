import * as $foreign from "./foreign.js";
export function sumRange(start) {
  return (end) => {
    return () => {
      const total = { value: 0 | 0 };
      const $start = traced("start")(start);
      const $end = traced("end")(end);
      for (let index = $start; index < $end; index++) {
        total.value = total.value + index | 0;
      }
      return total.value;
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
  const total = { value: 1 | 0 };
  for (let index = 1 | 0; index < end; index++) {
    total.value = total.value * index | 0;
  }
  return total.value;
}
export const traced = $foreign["traced"];
