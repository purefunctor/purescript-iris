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
export function union(record) {
  return {
    x: 1 | 0,
    ...record
  };
}
export function disjointUnion(record) {
  return {
    ...record,
    x: 1 | 0
  };
}
export function withDefaultsOrder(options) {
  const record = observe("options")(options);
  return {
    x: observe("default")(1 | 0),
    y: "default",
    ...record
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
export function overrideBranchOrder(record) {
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
  return {
    ...record,
    y: $field,
    x: $result
  };
}
export const observe = $foreign["observe"];
export const readTrace = $foreign["readTrace"];
export const literals = {
  y: "right",
  x: 1 | 0
};
