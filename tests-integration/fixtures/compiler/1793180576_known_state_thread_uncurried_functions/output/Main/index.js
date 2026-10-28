import * as Control_Monad_ST_Internal from "../Control.Monad.ST.Internal/index.js";
export function addToReference(reference, amount) {
  const section59 = reference.value;
  const $value = section59 + amount | 0;
  return reference.value = $value;
}
export function addTwice(value) {
  const $function = Control_Monad_ST_Internal.run;
  const $effect = () => {
    const reference = { value };
    const $int = addToReference(reference, 2 | 0);
    return addToReference(reference, 3 | 0);
  };
  return $function($effect);
}
