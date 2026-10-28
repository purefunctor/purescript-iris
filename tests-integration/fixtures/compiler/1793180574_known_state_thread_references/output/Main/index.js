import * as Control_Monad_ST_Internal from "../Control.Monad.ST.Internal/index.js";
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
    const section120 = counter.value;
    const $value = section120 + (1 | 0) | 0;
    return counter.value = $value;
  };
}
export function countTwice(value) {
  const $function = Control_Monad_ST_Internal.run;
  const $effect = () => {
    let counter = value;
    let $int;
    const section156 = counter;
    const $value = section156 * (2 | 0) | 0;
    $int = counter = $value;
    const $value$1 = value + (1 | 0) | 0;
    const written = counter = $value$1;
    let next;
    const section184 = counter;
    const $value$2 = section184 * (10 | 0) | 0;
    next = counter = $value$2;
    const current = counter;
    const $value$3 = (written + next | 0) + current | 0;
    return $value$3;
  };
  return $function($effect);
}
