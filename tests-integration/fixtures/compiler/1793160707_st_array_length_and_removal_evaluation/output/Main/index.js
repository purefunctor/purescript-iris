import * as $foreign from "./foreign.js";
export function removals(values) {
  const array = values.slice();
  const $array = traced("length array")(array);
  const length = $array.length;
  const $array$1 = traced("pop array")(array);
  const last = $array$1.length > 0 ? {
    tag: "Just",
    _1: $array$1.pop()
  } : "Nothing";
  const $array$2 = traced("shift array")(array);
  const first = $array$2.length > 0 ? {
    tag: "Just",
    _1: $array$2.shift()
  } : "Nothing";
  const final = array.slice();
  const $value = {
    length,
    last,
    first,
    final
  };
  return $value;
}
export const traced = $foreign["traced"];
