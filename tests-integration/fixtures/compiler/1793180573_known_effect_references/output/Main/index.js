import * as Data_Semiring from "../Data.Semiring/index.js";
export function newCounter(value) {
  return () => {
    return { value };
  };
}
export function readCounter(counter) {
  return () => {
    return counter.value;
  };
}
export function resetCounter(counter) {
  return () => {
    return counter.value = 0 | 0;
  };
}
export function incrementCounter(counter) {
  return () => {
    const section98 = counter.value;
    const $value = section98 + (1 | 0) | 0;
    return counter.value = $value;
  };
}
export function addToField(step) {
  return (state) => {
    const modifyFunction = /* @__PURE__ */ Data_Semiring.add(Data_Semiring.semiringInt)(step);
    const modifyReference = state.counter;
    return () => {
      const value = modifyReference.value;
      const $value = modifyFunction(value);
      return modifyReference.value = $value;
    };
  };
}
export function countTwice(value) {
  return () => {
    const counter = { value };
    let $int;
    const section163 = counter.value;
    const $value = section163 * (2 | 0) | 0;
    $int = counter.value = $value;
    const $value$1 = value + (1 | 0) | 0;
    const $unit = counter.value = $value$1;
    let next;
    const section190 = counter.value;
    const $value$2 = section190 * (10 | 0) | 0;
    next = counter.value = $value$2;
    const current = counter.value;
    const $value$3 = next + current | 0;
    return $value$3;
  };
}
