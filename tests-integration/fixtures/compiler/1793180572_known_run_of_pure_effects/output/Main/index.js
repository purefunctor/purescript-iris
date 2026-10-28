import * as Control_Monad_ST_Internal from "../Control.Monad.ST.Internal/index.js";
export function performPure(value) {
  return value + (1 | 0) | 0;
}
export function runPure(value) {
  return value * (2 | 0) | 0;
}
export function runReference(value) {
  const $function = Control_Monad_ST_Internal.run;
  const $action = Control_Monad_ST_Internal.new(value);
  const $effect = () => {
    const bindValue = $action();
    return Control_Monad_ST_Internal.read(bindValue)();
  };
  return $function($effect);
}
