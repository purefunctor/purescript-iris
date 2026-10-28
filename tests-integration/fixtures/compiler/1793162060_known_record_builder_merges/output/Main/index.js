import * as Control_Semigroupoid from "../Control.Semigroupoid/index.js";
import * as Record_Builder from "../Record.Builder/index.js";
export function merge(record) {
  return Record_Builder.build(/* @__PURE__ */ Record_Builder.merge({})({})({
    x: 0 | 0,
    z: true
  }))(record);
}
export function union(record) {
  return Record_Builder.build(/* @__PURE__ */ Record_Builder.union({})({ z: true }))(record);
}
export function disjointUnion(record) {
  return Record_Builder.build(/* @__PURE__ */ Record_Builder.disjointUnion({})({})({ z: true }))(record);
}
export function records(left) {
  return (right) => {
    return Record_Builder.build(/* @__PURE__ */ Record_Builder.merge({})({})(left))(right);
  };
}
export function insertThenMerge(record) {
  return Record_Builder.build(/* @__PURE__ */ Control_Semigroupoid.composeFlipped(Record_Builder.semigroupoidBuilder)(/* @__PURE__ */ Record_Builder.insert({})({})({ reflectSymbol: ($proxy) => "z" })("Proxy")(true))(/* @__PURE__ */ Record_Builder.merge({})({})({ x: 0 | 0 })))(record);
}
export const literals = Record_Builder.build(/* @__PURE__ */ Record_Builder.merge({})({})({ x: 0 | 0 }))({
  y: "y",
  x: 1 | 0
});
