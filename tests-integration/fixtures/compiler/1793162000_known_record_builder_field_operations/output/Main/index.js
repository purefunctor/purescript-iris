import * as Record_Builder from "../Record.Builder/index.js";
export function insertFlipped(record) {
  return {
    ...record,
    y: "y",
    z: true
  };
}
export function insertComposed(record) {
  return {
    ...record,
    y: "y",
    z: true
  };
}
export function modify(record) {
  return {
    ...record,
    x: ((x) => x + (1 | 0) | 0)(record.x)
  };
}
function $delete(record) {
  const { x: $deleted, ...$record } = record;
  return $record;
}
export function rename(record) {
  const { x: $deleted, ...$record } = record;
  return {
    ...$record,
    z: record.x
  };
}
export function pipeline(record) {
  const record$2 = {
    ...record,
    z: true
  };
  const $result = {
    ...record$2,
    x: ((x) => x * (2 | 0) | 0)(record$2.x)
  };
  const { y: $deleted, ...$record } = $result;
  const record$1 = $record;
  const { x: $deleted$1, ...$record$1 } = record$1;
  return {
    ...$record$1,
    w: record$1.x
  };
}
export function unknown(builder) {
  return (record) => {
    return Record_Builder.build(builder)(record);
  };
}
export const fromScratch = {
  x: 1 | 0,
  y: "y"
};
export const literal = {
  z: true,
  w: ((x) => x * (2 | 0) | 0)(1 | 0)
};
export { $delete as "delete" };
