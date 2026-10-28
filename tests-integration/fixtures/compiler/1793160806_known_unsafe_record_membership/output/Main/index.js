import * as Record_Unsafe from "../Record.Unsafe/index.js";
import * as $foreign from "./foreign.js";
export function presentOrder($boolean) {
  return Record_Unsafe.unsafeHas("y")({
    x: observe("x")(1 | 0),
    y: observe("y")("y")
  });
}
export function unknown(record) {
  return Record_Unsafe.unsafeHas("x")(record);
}
export const observe = $foreign["observe"];
export const readTrace = $foreign["readTrace"];
export const present = Record_Unsafe.unsafeHas("x")({
  x: 1 | 0,
  y: "y"
});
export const absent = Record_Unsafe.unsafeHas("z")({
  x: 1 | 0,
  y: "y"
});
