import * as Control_Category from "../Control.Category/index.js";
import * as Control_Semigroupoid from "../Control.Semigroupoid/index.js";
import * as Record_Builder from "../Record.Builder/index.js";
export function insertFlipped(record) {
  return Record_Builder.build(/* @__PURE__ */ Control_Semigroupoid.composeFlipped(Record_Builder.semigroupoidBuilder)(/* @__PURE__ */ Record_Builder.insert({})({})({ reflectSymbol: ($proxy) => "y" })("Proxy")("y"))(/* @__PURE__ */ Record_Builder.insert({})({})({ reflectSymbol: ($proxy) => "z" })("Proxy")(true)))(record);
}
export function insertComposed(record) {
  return Record_Builder.build(/* @__PURE__ */ Control_Semigroupoid.compose(Record_Builder.semigroupoidBuilder)(/* @__PURE__ */ Record_Builder.insert({})({})({ reflectSymbol: ($proxy) => "z" })("Proxy")(true))(/* @__PURE__ */ Record_Builder.insert({})({})({ reflectSymbol: ($proxy) => "y" })("Proxy")("y")))(record);
}
export function modify(record) {
  return Record_Builder.build(/* @__PURE__ */ Record_Builder.modify({})({})({ reflectSymbol: ($proxy) => "x" })("Proxy")((x) => x + (1 | 0) | 0))(record);
}
function $delete(record) {
  return Record_Builder.build(/* @__PURE__ */ Record_Builder.delete({ reflectSymbol: ($proxy) => "x" })({})({})("Proxy"))(record);
}
export function rename(record) {
  return Record_Builder.build(/* @__PURE__ */ Record_Builder.rename({ reflectSymbol: ($proxy) => "x" })({ reflectSymbol: ($proxy) => "z" })({})({})({})({})("Proxy")("Proxy"))(record);
}
export function pipeline(record) {
  return Record_Builder.build(/* @__PURE__ */ Control_Semigroupoid.composeFlipped(Record_Builder.semigroupoidBuilder)(/* @__PURE__ */ Record_Builder.insert({})({})({ reflectSymbol: ($proxy) => "z" })("Proxy")(true))(/* @__PURE__ */ Control_Semigroupoid.composeFlipped(Record_Builder.semigroupoidBuilder)(/* @__PURE__ */ Record_Builder.modify({})({})({ reflectSymbol: ($proxy) => "x" })("Proxy")((x) => x * (2 | 0) | 0))(/* @__PURE__ */ Control_Semigroupoid.composeFlipped(Record_Builder.semigroupoidBuilder)(/* @__PURE__ */ Control_Category.identity(Record_Builder.categoryBuilder))(/* @__PURE__ */ Control_Semigroupoid.composeFlipped(Record_Builder.semigroupoidBuilder)(/* @__PURE__ */ Record_Builder.delete({ reflectSymbol: ($proxy) => "y" })({})({})("Proxy"))(/* @__PURE__ */ Control_Semigroupoid.composeFlipped(Record_Builder.semigroupoidBuilder)(/* @__PURE__ */ Record_Builder.rename({ reflectSymbol: ($proxy) => "x" })({ reflectSymbol: ($proxy) => "w" })({})({})({})({})("Proxy")("Proxy"))(/* @__PURE__ */ Record_Builder.nub({})))))))(record);
}
export function unknown(builder) {
  return (record) => {
    return Record_Builder.build(builder)(record);
  };
}
export const fromScratch = Record_Builder.buildFromScratch(/* @__PURE__ */ Control_Semigroupoid.composeFlipped(Record_Builder.semigroupoidBuilder)(/* @__PURE__ */ Record_Builder.insert({})({})({ reflectSymbol: ($proxy) => "x" })("Proxy")(1 | 0))(/* @__PURE__ */ Record_Builder.insert({})({})({ reflectSymbol: ($proxy) => "y" })("Proxy")("y")));
export const literal = Record_Builder.build(/* @__PURE__ */ Control_Semigroupoid.composeFlipped(Record_Builder.semigroupoidBuilder)(/* @__PURE__ */ Record_Builder.insert({})({})({ reflectSymbol: ($proxy) => "z" })("Proxy")(true))(/* @__PURE__ */ Control_Semigroupoid.composeFlipped(Record_Builder.semigroupoidBuilder)(/* @__PURE__ */ Record_Builder.modify({})({})({ reflectSymbol: ($proxy) => "x" })("Proxy")((x) => x * (2 | 0) | 0))(/* @__PURE__ */ Control_Semigroupoid.composeFlipped(Record_Builder.semigroupoidBuilder)(/* @__PURE__ */ Record_Builder.delete({ reflectSymbol: ($proxy) => "y" })({})({})("Proxy"))(/* @__PURE__ */ Record_Builder.rename({ reflectSymbol: ($proxy) => "x" })({ reflectSymbol: ($proxy) => "w" })({})({})({})({})("Proxy")("Proxy")))))({
  x: 1 | 0,
  y: "y"
});
export { $delete as "delete" };
