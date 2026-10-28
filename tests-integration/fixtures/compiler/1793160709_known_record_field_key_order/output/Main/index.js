export function set(record) {
  return {
    ...record,
    x: "set"
  };
}
export function setLiteral(y) {
  return {
    ...{
      y,
      x: 0 | 0
    },
    x: "set"
  };
}
export function insert(record) {
  return {
    ...record,
    x: 1 | 0
  };
}
export function insertLiteral(y) {
  return {
    ...{
      z: 2 | 0,
      y
    },
    x: 1 | 0
  };
}
export function modify(record) {
  return {
    ...record,
    x: ((section170) => section170 + (1 | 0) | 0)(record.x)
  };
}
export function modifyLiteral(y) {
  const record = {
    y,
    x: 0 | 0
  };
  return {
    ...record,
    x: ((section202) => section202 + (1 | 0) | 0)(record.x)
  };
}
function $delete(record) {
  const { x: $deleted, ...$record } = record;
  return $record;
}
export function deleteLiteral(y) {
  return {
    z: 2 | 0,
    y
  };
}
export { $delete as "delete" };
