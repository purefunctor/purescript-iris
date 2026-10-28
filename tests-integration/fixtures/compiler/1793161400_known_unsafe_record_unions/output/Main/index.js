import * as Record_Unsafe_Union from "../Record.Unsafe.Union/index.js";
import * as $foreign from "./foreign.js";
export function withDefaults(options) {
  return Record_Unsafe_Union.unsafeUnion(options)({
    x: 1 | 0,
    y: "default"
  });
}
export function override(record) {
  return Record_Unsafe_Union.unsafeUnionFn({ x: 0 | 0 }, record);
}
export function records(left) {
  return (right) => {
    return Record_Unsafe_Union.unsafeUnionFn(left, right);
  };
}
export function overrideOrder(record) {
  return Record_Unsafe_Union.unsafeUnion({ x: observe("override")(0 | 0) })(observe("record")(record));
}
export function literalsOrder($boolean) {
  return Record_Unsafe_Union.unsafeUnionFn({ x: observe("left")(1 | 0) }, {
    y: "right",
    x: observe("right")(2 | 0)
  });
}
export const observe = $foreign["observe"];
export const readTrace = $foreign["readTrace"];
export const literals = Record_Unsafe_Union.unsafeUnionFn({ x: 1 | 0 }, {
  y: "right",
  x: 2 | 0
});
