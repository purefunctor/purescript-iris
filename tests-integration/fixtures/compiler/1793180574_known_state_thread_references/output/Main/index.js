import * as Control_Monad_ST_Internal from "../Control.Monad.ST.Internal/index.js";
export function newCounter(value) {
  return Control_Monad_ST_Internal.new(value);
}
export function readCounter(counter) {
  return Control_Monad_ST_Internal.read(counter);
}
export function resetCounter(counter) {
  return Control_Monad_ST_Internal.write(0 | 0)(counter);
}
export function incrementCounter(counter) {
  return Control_Monad_ST_Internal.modify((section120) => section120 + (1 | 0) | 0)(counter);
}
export function countTwice(value) {
  const $function = Control_Monad_ST_Internal.run;
  const $action = Control_Monad_ST_Internal.new(value);
  const $effect = () => {
    const counter = $action();
    const $action$1 = Control_Monad_ST_Internal.modify((section156) => section156 * (2 | 0) | 0)(counter);
    const $int = $action$1();
    const $action$2 = Control_Monad_ST_Internal.write(value + (1 | 0) | 0)(counter);
    const written = $action$2();
    const $action$3 = Control_Monad_ST_Internal.modify((section184) => section184 * (10 | 0) | 0)(counter);
    const next = $action$3();
    const $action$4 = Control_Monad_ST_Internal.read(counter);
    const current = $action$4();
    const $value = (written + next | 0) + current | 0;
    return $value;
  };
  return $function($effect);
}
