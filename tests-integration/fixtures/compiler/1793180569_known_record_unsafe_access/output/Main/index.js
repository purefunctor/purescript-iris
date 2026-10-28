import * as Record_Unsafe from "../Record.Unsafe/index.js";
export function readX(point) {
  return Record_Unsafe.unsafeGet("x")(point);
}
export function readSpacedLabel(person) {
  return Record_Unsafe.unsafeGet("first name")(person);
}
export function setX(value) {
  return (point) => {
    return Record_Unsafe.unsafeSet("x")(value)(point);
  };
}
export function addZ(value) {
  return (point) => {
    return Record_Unsafe.unsafeSet("z")(value)(point);
  };
}
export function readLabel(label) {
  return (point) => {
    return Record_Unsafe.unsafeGet(label)(point);
  };
}
