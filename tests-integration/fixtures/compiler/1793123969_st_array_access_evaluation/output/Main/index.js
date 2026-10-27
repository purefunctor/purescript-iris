import * as Data_Array_ST from "../Data.Array.ST/index.js";
import * as $foreign from "./foreign.js";
export function accesses(values) {
  const array = values.slice();
  const $action = Data_Array_ST.peek(traced("peek index")(1 | 0))(traced("peek array")(array));
  const peeked = $action();
  const $action$1 = Data_Array_ST.poke(traced("poke index")(5 | 0))(traced("poke value")(9 | 0))(traced("poke array")(array));
  const poked = $action$1();
  const final = array.slice();
  const $value = {
    peeked,
    poked,
    final
  };
  return $value;
}
export const traced = $foreign["traced"];
