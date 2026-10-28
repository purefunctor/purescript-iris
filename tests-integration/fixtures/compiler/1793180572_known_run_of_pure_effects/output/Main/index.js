import * as Control_Monad_ST_Internal from "../Control.Monad.ST.Internal/index.js";
export function performPure(value) {
  return value + (1 | 0) | 0;
}
export function runPure(value) {
  return value * (2 | 0) | 0;
}
export function runReference(value) {
  const $function = Control_Monad_ST_Internal.run;
  const $effect = () => {
    const bindValue = { value };
    return bindValue.value;
  };
  return $function($effect);
}
