import * as Control_Semigroupoid from "../Control.Semigroupoid/index.js";
import * as Record_Builder from "../Record.Builder/index.js";
export function withZ(value) {
  return (point) => {
    return Record_Builder.build(/* @__PURE__ */ Record_Builder.insert({})({})({ reflectSymbol: ($proxy) => "z" })("Proxy")(value))(point);
  };
}
export function shifted(point) {
  return Record_Builder.build(/* @__PURE__ */ Control_Semigroupoid.composeFlipped(Record_Builder.semigroupoidBuilder)(/* @__PURE__ */ Record_Builder.modify({})({})({ reflectSymbol: ($proxy) => "x" })("Proxy")((section91) => section91 + (1 | 0) | 0))(/* @__PURE__ */ Record_Builder.modify({})({})({ reflectSymbol: ($proxy) => "y" })("Proxy")((section109) => section109 * (2 | 0) | 0)))(point);
}
export function insertedThenModified(point) {
  return Record_Builder.build(/* @__PURE__ */ Control_Semigroupoid.composeFlipped(Record_Builder.semigroupoidBuilder)(/* @__PURE__ */ Record_Builder.insert({})({})({ reflectSymbol: ($proxy) => "z" })("Proxy")(5 | 0))(/* @__PURE__ */ Record_Builder.modify({})({})({ reflectSymbol: ($proxy) => "z" })("Proxy")((section160) => section160 + (1 | 0) | 0)))(point);
}
export function mergedOnto(point) {
  return Record_Builder.build(/* @__PURE__ */ Record_Builder.merge({})({})({
    y: 20 | 0,
    z: 30 | 0
  }))(point);
}
export function withoutY(point) {
  return Record_Builder.build(/* @__PURE__ */ Record_Builder.delete({ reflectSymbol: ($proxy) => "y" })({})({})("Proxy"))(point);
}
export function unknownBuilder(builder) {
  return (point) => {
    return Record_Builder.build(builder)(point);
  };
}
