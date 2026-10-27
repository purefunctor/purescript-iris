import * as $foreign from "./foreign.js";
export function accesses(values) {
  const array = values.slice();
  const $index = traced("peek index")(1 | 0);
  const $array = traced("peek array")(array);
  const peeked = $index >= 0 && $index < $array.length ? {
    tag: "Just",
    _1: $array[$index]
  } : "Nothing";
  const $index$1 = traced("poke index")(5 | 0);
  const $value = traced("poke value")(9 | 0);
  const $array$1 = traced("poke array")(array);
  const $inBounds = $index$1 >= 0 && $index$1 < $array$1.length;
  if ($inBounds) {
    $array$1[$index$1] = $value;
  }
  const poked = $inBounds;
  const final = array.slice();
  const $value$1 = {
    peeked,
    poked,
    final
  };
  return $value$1;
}
export const traced = $foreign["traced"];
