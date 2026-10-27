import * as $foreign from "./foreign.js";
function $delete(record) {
  const { x: $deleted, ...$record } = record;
  return $record;
}
export function deleteQuoted(record) {
  const { "first name": $deleted, ...$record } = record;
  return $record;
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
