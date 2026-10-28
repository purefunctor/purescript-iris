import * as Record_Unsafe from "../Record.Unsafe/index.js";
export function hasX(point) {
  return Record_Unsafe.unsafeHas("x")(point);
}
export function literalDeleteY(value) {
  return { x: value };
}
export function deleteY(point) {
  return Record_Unsafe.unsafeDelete("y")(point);
}
export const literalHasX = true;
export const literalHasZ = false;
