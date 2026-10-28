import * as Control_Semigroupoid from "../Control.Semigroupoid/index.js";
import * as Record_Builder from "../Record.Builder/index.js";
import * as $foreign from "./foreign.js";
export function flipped(record) {
  return Record_Builder.build(/* @__PURE__ */ Control_Semigroupoid.composeFlipped(Record_Builder.semigroupoidBuilder)(/* @__PURE__ */ Record_Builder.insert({})({})({ reflectSymbol: ($proxy) => "y" })("Proxy")(observe("y")("y")))(/* @__PURE__ */ Control_Semigroupoid.composeFlipped(Record_Builder.semigroupoidBuilder)(/* @__PURE__ */ Record_Builder.modify({})({})({ reflectSymbol: ($proxy) => "x" })("Proxy")(observe("modify")((x) => observe("apply")(x + (1 | 0) | 0))))(/* @__PURE__ */ Record_Builder.merge({})({})({ z: observe("z")(true) }))))(observe("record")(record));
}
export function composed(record) {
  return Record_Builder.build(/* @__PURE__ */ Control_Semigroupoid.compose(Record_Builder.semigroupoidBuilder)(/* @__PURE__ */ Record_Builder.insert({})({})({ reflectSymbol: ($proxy) => "y" })("Proxy")(observe("y")("y")))(/* @__PURE__ */ Record_Builder.modify({})({})({ reflectSymbol: ($proxy) => "x" })("Proxy")((x) => observe("apply")(x + (1 | 0) | 0))))(observe("record")(record));
}
export function literal($boolean) {
  return Record_Builder.build(/* @__PURE__ */ Control_Semigroupoid.composeFlipped(Record_Builder.semigroupoidBuilder)(/* @__PURE__ */ Record_Builder.modify({})({})({ reflectSymbol: ($proxy) => "y" })("Proxy")((y) => observe("first")(y + (1 | 0) | 0)))(/* @__PURE__ */ Control_Semigroupoid.composeFlipped(Record_Builder.semigroupoidBuilder)(/* @__PURE__ */ Record_Builder.modify({})({})({ reflectSymbol: ($proxy) => "x" })("Proxy")((x) => observe("second")(x + (1 | 0) | 0)))(/* @__PURE__ */ Record_Builder.delete({ reflectSymbol: ($proxy) => "z" })({})({})("Proxy"))))({
    x: observe("x")(1 | 0),
    y: observe("y")(2 | 0),
    z: observe("z")(3 | 0)
  });
}
export const observe = $foreign["observe"];
export const readTrace = $foreign["readTrace"];
