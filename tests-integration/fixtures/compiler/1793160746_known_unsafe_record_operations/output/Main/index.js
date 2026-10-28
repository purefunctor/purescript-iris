import * as $foreign from "./foreign.js";
export function get(record) {
  return record.x;
}
export function getQuoted(record) {
  return record["first name"];
}
export function set(record) {
  return {
    ...record,
    x: 2 | 0
  };
}
export function setNew(record) {
  return {
    ...record,
    x: 2 | 0
  };
}
function $delete(record) {
  const { x: $deleted, ...$record } = record;
  return $record;
}
export function deleteLiteral($boolean) {
  const x = observe("x")(1 | 0);
  return { y: observe("y")("y") };
}
export function setOrder(record) {
  const value = observe("value")(2 | 0);
  return {
    ...observe("record")(record),
    x: value
  };
}
export function deleteOrder(record) {
  const $field = observe("first")(1 | 0);
  const { x: $deleted, ...$record } = observe("record")(record);
  return {
    first: $field,
    rest: $record
  };
}
export const observe = $foreign["observe"];
export const readTrace = $foreign["readTrace"];
export { $delete as "delete" };
