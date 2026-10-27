import * as Record from "../Record/index.js";
import * as $foreign from "./foreign.js";
export function get(record) {
  return /* @__PURE__ */ Record.get({ reflectSymbol: ($proxy) => "x" })({})("Proxy")(record);
}
export function getQuoted(record) {
  return /* @__PURE__ */ Record.get({ reflectSymbol: ($proxy) => "first name" })({})("Proxy")(record);
}
export function set(record) {
  return /* @__PURE__ */ Record.set({ reflectSymbol: ($proxy) => "x" })({})({})("Proxy")("set")(record);
}
export function modify(record) {
  return /* @__PURE__ */ Record.modify({ reflectSymbol: ($proxy) => "x" })({})({})("Proxy")((section151) => section151 + (1 | 0) | 0)(record);
}
export function insert(record) {
  return /* @__PURE__ */ Record.insert({ reflectSymbol: ($proxy) => "x" })({})({})("Proxy")(1 | 0)(record);
}
export function setOrder(record) {
  return /* @__PURE__ */ Record.set({ reflectSymbol: ($proxy) => "x" })({})({})("Proxy")(observe("value")("set"))(observe("record")(record));
}
export function modifyOrder(record) {
  return /* @__PURE__ */ Record.modify({ reflectSymbol: ($proxy) => "x" })({})({})("Proxy")(observe("function")((section265) => section265 + (1 | 0) | 0))(observe("record")(record));
}
export function getPolymorphic(isSymbolLabelDict) {
  return (consLabelValueRestRowDict) => {
    return (label) => (record) => /* @__PURE__ */ Record.get(isSymbolLabelDict)(consLabelValueRestRowDict)(label)(record);
  };
}
export const observe = $foreign["observe"];
export const readTrace = $foreign["readTrace"];
