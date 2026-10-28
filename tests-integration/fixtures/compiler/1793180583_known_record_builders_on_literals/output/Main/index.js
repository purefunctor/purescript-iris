import * as Control_Semigroupoid from "../Control.Semigroupoid/index.js";
import * as Record_Builder from "../Record.Builder/index.js";
export function inserted(value) {
  return Record_Builder.build(/* @__PURE__ */ Record_Builder.insert({})({})({ reflectSymbol: ($proxy) => "z" })("Proxy")(value))({
    x: 1 | 0,
    y: 2 | 0
  });
}
export function reshaped(value) {
  return Record_Builder.build(/* @__PURE__ */ Control_Semigroupoid.composeFlipped(Record_Builder.semigroupoidBuilder)(/* @__PURE__ */ Record_Builder.modify({})({})({ reflectSymbol: ($proxy) => "x" })("Proxy")((section93) => section93 + value | 0))(/* @__PURE__ */ Control_Semigroupoid.composeFlipped(Record_Builder.semigroupoidBuilder)(/* @__PURE__ */ Record_Builder.delete({ reflectSymbol: ($proxy) => "y" })({})({})("Proxy"))(/* @__PURE__ */ Control_Semigroupoid.composeFlipped(Record_Builder.semigroupoidBuilder)(/* @__PURE__ */ Record_Builder.rename({ reflectSymbol: ($proxy) => "v" })({ reflectSymbol: ($proxy) => "w" })({})({})({})({})("Proxy")("Proxy"))(/* @__PURE__ */ Record_Builder.insert({})({})({ reflectSymbol: ($proxy) => "z" })("Proxy")("new")))))({
    x: 1 | 0,
    y: 2 | 0,
    v: 3 | 0
  });
}
export function fromScratch(value) {
  return Record_Builder.buildFromScratch(/* @__PURE__ */ Control_Semigroupoid.composeFlipped(Record_Builder.semigroupoidBuilder)(/* @__PURE__ */ Record_Builder.insert({})({})({ reflectSymbol: ($proxy) => "x" })("Proxy")(value))(/* @__PURE__ */ Record_Builder.insert({})({})({ reflectSymbol: ($proxy) => "y" })("Proxy")(2 | 0)));
}
export const composedBackwards = Record_Builder.build(/* @__PURE__ */ Control_Semigroupoid.compose(Record_Builder.semigroupoidBuilder)(/* @__PURE__ */ Record_Builder.modify({})({})({ reflectSymbol: ($proxy) => "y" })("Proxy")((section173) => section173 * (10 | 0) | 0))(/* @__PURE__ */ Record_Builder.insert({})({})({ reflectSymbol: ($proxy) => "y" })("Proxy")(4 | 0)))({ x: 1 | 0 });
export const merged = Record_Builder.build(/* @__PURE__ */ Record_Builder.merge({})({})({
  y: 20 | 0,
  z: 30 | 0
}))({
  x: 1 | 0,
  y: 2 | 0
});
