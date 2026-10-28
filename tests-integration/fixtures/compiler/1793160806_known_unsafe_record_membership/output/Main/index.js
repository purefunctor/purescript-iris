import * as Record_Unsafe from "../Record.Unsafe/index.js";
import * as $foreign from "./foreign.js";
export function presentOrder($boolean) {
  const x = observe("x")(1 | 0);
  const y = observe("y")("y");
  return true;
}
export function unknown(record) {
  return Record_Unsafe.unsafeHas("x")(record);
}
export const observe = $foreign["observe"];
export const readTrace = $foreign["readTrace"];
export const present = true;
export const absent = false;
