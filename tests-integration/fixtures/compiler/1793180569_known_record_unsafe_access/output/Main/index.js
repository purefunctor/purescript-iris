import * as Record_Unsafe from "../Record.Unsafe/index.js";
export function readX(point) {
  return point.x;
}
export function readSpacedLabel(person) {
  return person["first name"];
}
export function setX(value) {
  return (point) => {
    return {
      ...point,
      x: value
    };
  };
}
export function addZ(value) {
  return (point) => {
    return {
      ...point,
      z: value
    };
  };
}
export function readLabel(label) {
  return (point) => {
    return Record_Unsafe.unsafeGet(label)(point);
  };
}
