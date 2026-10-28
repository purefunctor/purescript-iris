import * as Data_Array_ST from "../Data.Array.ST/index.js";
import * as $foreign from "./foreign.js";
export function modifyAt(index) {
  return ($function) => {
    return (array) => {
      return Data_Array_ST.modify(traced("index")(index))(traced("function")($function))(traced("array")(array));
    };
  };
}
export const traced = $foreign["traced"];
