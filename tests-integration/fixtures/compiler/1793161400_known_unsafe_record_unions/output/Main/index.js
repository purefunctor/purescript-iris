import * as Record_Unsafe_Union from "../Record.Unsafe.Union/index.js";
import * as $foreign from "./foreign.js";
export function withDefaults(options) {
  return {
    x: 1 | 0,
    y: "default",
    ...options
  };
}
export function override(record) {
  return {
    ...record,
    x: 0 | 0
  };
}
export function records(left) {
  return (right) => {
    return Record_Unsafe_Union.unsafeUnionFn(left, right);
  };
}
export function overrideOrder(record) {
  const x = observe("override")(0 | 0);
  return {
    ...observe("record")(record),
    x
  };
}
export function literalsOrder($boolean) {
  const x = observe("left")(1 | 0);
  const x$1 = observe("right")(2 | 0);
  return {
    y: "right",
    x
  };
}
export const observe = $foreign["observe"];
export const readTrace = $foreign["readTrace"];
export const literals = {
  y: "right",
  x: 1 | 0
};
