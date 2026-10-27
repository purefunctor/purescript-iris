import * as Record from "../Record/index.js";
import * as $foreign from "./foreign.js";
export function withDefaults(options) {
  return /* @__PURE__ */ Record.merge({})({})(options)({
    x: 1 | 0,
    y: "default"
  });
}
export function override(record) {
  return /* @__PURE__ */ Record.merge({})({})({ x: 0 | 0 })(record);
}
export function union(record) {
  return /* @__PURE__ */ Record.union({})(record)({ x: 1 | 0 });
}
export function disjointUnion(record) {
  return /* @__PURE__ */ Record.disjointUnion({})({})({ x: 1 | 0 })(record);
}
export function withDefaultsOrder(options) {
  return /* @__PURE__ */ Record.merge({})({})(observe("options")(options))({
    x: observe("default")(1 | 0),
    y: "default"
  });
}
export function overrideOrder(record) {
  return /* @__PURE__ */ Record.merge({})({})({ x: observe("override")(0 | 0) })(observe("record")(record));
}
export function literalsOrder($boolean) {
  return /* @__PURE__ */ Record.merge({})({})({ x: observe("left")(1 | 0) })({
    y: "right",
    x: observe("right")(2 | 0)
  });
}
export function overrideBranchOrder(record) {
  const $function = Record.merge({})({});
  const $field = observe("override")("y");
  let $result;
  const $scrutinee = record.y;
  $case: {
    if ($scrutinee === "branch") {
      $result = observe("branch")(1 | 0);
      break $case;
    }
    $result = observe("fallback")(2 | 0);
    break $case;
  }
  return /* @__PURE__ */ $function({
    y: $field,
    x: $result
  })(record);
}
export const observe = $foreign["observe"];
export const readTrace = $foreign["readTrace"];
export const literals = /* @__PURE__ */ Record.merge({})({})({ x: 1 | 0 })({
  y: "right",
  x: 2 | 0
});
