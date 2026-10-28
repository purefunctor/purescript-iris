import * as Record_Unsafe from "../Record.Unsafe/index.js";
export function hasX(point) {
  return Record_Unsafe.unsafeHas("x")(point);
}
export function literalDeleteY(value) {
  return Record_Unsafe.unsafeDelete("y")({
    x: value,
    y: 2 | 0
  });
}
export function deleteY(point) {
  return Record_Unsafe.unsafeDelete("y")(point);
}
export const literalHasX = Record_Unsafe.unsafeHas("x")({
  x: 1 | 0,
  y: 2 | 0
});
export const literalHasZ = Record_Unsafe.unsafeHas("z")({
  x: 1 | 0,
  y: 2 | 0
});
