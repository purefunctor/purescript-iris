import * as $foreign from "./foreign.js";
export function flipped(record) {
  const value = observe("y")("y");
  const modify = observe("modify")((x) => observe("apply")(x + (1 | 0) | 0));
  const z = observe("z")(true);
  const record$1 = observe("record")(record);
  const record$2 = {
    ...record$1,
    y: value
  };
  const $result = {
    ...record$2,
    x: modify(record$2.x)
  };
  return {
    ...$result,
    z
  };
}
export function composed(record) {
  const value = observe("y")("y");
  const record$1 = observe("record")(record);
  return {
    ...record$1,
    x: ((x) => observe("apply")(x + (1 | 0) | 0))(record$1.x),
    y: value
  };
}
export function literal($boolean) {
  const x = observe("x")(1 | 0);
  const y = observe("y")(2 | 0);
  const z = observe("z")(3 | 0);
  const record = {
    x,
    y: ((y$1) => observe("first")(y$1 + (1 | 0) | 0))(y),
    z
  };
  const $result = {
    ...record,
    x: ((x$1) => observe("second")(x$1 + (1 | 0) | 0))(record.x)
  };
  const { z: $deleted, ...$record } = $result;
  return $record;
}
export const observe = $foreign["observe"];
export const readTrace = $foreign["readTrace"];
