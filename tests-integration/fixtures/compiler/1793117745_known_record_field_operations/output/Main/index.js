import * as Record from "../Record/index.js";
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
    x: "set"
  };
}
export function modify(record) {
  return {
    ...record,
    x: ((section151) => section151 + (1 | 0) | 0)(record.x)
  };
}
export function insert(record) {
  return {
    ...record,
    x: 1 | 0
  };
}
export function setOrder(record) {
  const value = observe("value")("set");
  return {
    ...observe("record")(record),
    x: value
  };
}
export function modifyOrder(record) {
  const modify$1 = observe("function")((section265) => section265 + (1 | 0) | 0);
  const record$1 = observe("record")(record);
  return {
    ...record$1,
    x: modify$1(record$1.x)
  };
}
export function getPolymorphic(isSymbolLabelDict) {
  return (consLabelValueRestRowDict) => {
    return (label) => (record) => /* @__PURE__ */ Record.get(isSymbolLabelDict)(consLabelValueRestRowDict)(label)(record);
  };
}
export const observe = $foreign["observe"];
export const readTrace = $foreign["readTrace"];
