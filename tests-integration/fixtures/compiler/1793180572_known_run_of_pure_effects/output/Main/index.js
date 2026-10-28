import * as Control_Monad_ST_Internal from "../Control.Monad.ST.Internal/index.js";
import * as Effect_Unsafe from "../Effect.Unsafe/index.js";
export function performPure(value) {
  const $function = Effect_Unsafe.unsafePerformEffect;
  const $value = value + (1 | 0) | 0;
  const $effect = () => {
    return $value;
  };
  return $function($effect);
}
export function runPure(value) {
  const $function = Control_Monad_ST_Internal.run;
  const $value = value * (2 | 0) | 0;
  const $effect = () => {
    return $value;
  };
  return $function($effect);
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
