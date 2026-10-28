import * as Record_Unsafe_Union from "../Record.Unsafe.Union/index.js";
export function literalUnion(value) {
  return Record_Unsafe_Union.unsafeUnion({
    x: value,
    y: 2 | 0
  })({
    y: 20 | 0,
    z: 30 | 0
  });
}
export function literalUnionFn(value) {
  return Record_Unsafe_Union.unsafeUnionFn({
    x: 1 | 0,
    y: value
  }, {
    y: 20 | 0,
    z: 30 | 0
  });
}
export function unionWithLiteral(right) {
  return Record_Unsafe_Union.unsafeUnion({
    x: 1 | 0,
    y: 2 | 0
  })(right);
}
