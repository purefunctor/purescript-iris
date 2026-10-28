import * as Record_Builder from "../Record.Builder/index.js";
export function withZ(value) {
  return (point) => {
    return {
      ...point,
      z: value
    };
  };
}
export function shifted(point) {
  return {
    ...point,
    x: point.x + (1 | 0) | 0,
    y: point.y * (2 | 0) | 0
  };
}
export function insertedThenModified(point) {
  return {
    ...point,
    z: (5 | 0) + (1 | 0) | 0
  };
}
export function mergedOnto(point) {
  return {
    ...point,
    y: 20 | 0,
    z: 30 | 0
  };
}
export function withoutY(point) {
  return Record_Builder.build(/* @__PURE__ */ Record_Builder.delete({ reflectSymbol: ($proxy) => "y" })({})({})("Proxy"))(point);
}
export function unknownBuilder(builder) {
  return (point) => {
    return Record_Builder.build(builder)(point);
  };
}
