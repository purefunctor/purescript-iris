import * as Record from "../Record/index.js";
import * as $foreign from "./foreign.js";
function $delete(record) {
  return /* @__PURE__ */ Record.delete({ reflectSymbol: ($proxy) => "x" })({})({})("Proxy")(record);
}
export function deleteQuoted(record) {
  return /* @__PURE__ */ Record.delete({ reflectSymbol: ($proxy) => "first name" })({})({})("Proxy")(record);
}
export function deleteOrder(record) {
  return {
    first: observe("first")(1 | 0),
    rest: /* @__PURE__ */ Record.delete({ reflectSymbol: ($proxy) => "x" })({})({})("Proxy")(observe("record")(record))
  };
}
export const observe = $foreign["observe"];
export const readTrace = $foreign["readTrace"];
export { $delete as "delete" };
