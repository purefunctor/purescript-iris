import * as Record_Unsafe from "../Record.Unsafe/index.js";
import * as $foreign from "./foreign.js";
export function get(record) {
  return Record_Unsafe.unsafeGet("x")(record);
}
export function getQuoted(record) {
  return Record_Unsafe.unsafeGet("first name")(record);
}
export function set(record) {
  return Record_Unsafe.unsafeSet("x")(2 | 0)(record);
}
export function setNew(record) {
  return Record_Unsafe.unsafeSet("x")(2 | 0)(record);
}
function $delete(record) {
  return Record_Unsafe.unsafeDelete("x")(record);
}
export function deleteLiteral($boolean) {
  return Record_Unsafe.unsafeDelete("x")({
    x: observe("x")(1 | 0),
    y: observe("y")("y")
  });
}
export function setOrder(record) {
  return Record_Unsafe.unsafeSet("x")(observe("value")(2 | 0))(observe("record")(record));
}
export function deleteOrder(record) {
  return {
    first: observe("first")(1 | 0),
    rest: Record_Unsafe.unsafeDelete("x")(observe("record")(record))
  };
}
export const observe = $foreign["observe"];
export const readTrace = $foreign["readTrace"];
export { $delete as "delete" };
