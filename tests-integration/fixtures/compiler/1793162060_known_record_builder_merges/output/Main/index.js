export function merge(record) {
  return {
    ...record,
    x: 0 | 0,
    z: true
  };
}
export function union(record) {
  return {
    ...record,
    z: true
  };
}
export function disjointUnion(record) {
  return {
    ...record,
    z: true
  };
}
export function records(left) {
  return (right) => {
    return {
      ...right,
      ...left
    };
  };
}
export function insertThenMerge(record) {
  return {
    ...{
      ...record,
      z: true
    },
    x: 0 | 0
  };
}
export const literals = {
  y: "y",
  x: 0 | 0
};
