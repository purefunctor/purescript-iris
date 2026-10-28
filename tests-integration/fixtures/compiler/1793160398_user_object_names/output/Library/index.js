import * as Data_Show from "../Data.Show/index.js";
const $Object = ($value0) => ({
  tag: "Object",
  _1: $value0
});
const $Array = ($value0) => ({
  tag: "Array",
  _1: $value0
});
const $Error = ($value0) => ({
  tag: "Error",
  _1: $value0
});
export function describe(value) {
  if (value.tag === "Object") {
    const { _1: object } = value;
    if ("a" in object) {
      return Object.keys(object);
    }
  }
  if (value.tag === "Object") {
    return [];
  }
  if (value.tag === "Array" && Array.isArray(value._1) && value._1.length === 2) {
    const first = value._1[0];
    const second = value._1[1];
    return [/* @__PURE__ */ Data_Show.show(Data_Show.showInt)(first + second | 0)];
  }
  if (value.tag === "Array") {
    return [];
  }
  if (value.tag === "Error") {
    const { _1: message } = value;
    return [message];
  }
  throw new Error("Pattern match failure");
}
export function objectSize(value) {
  if (value.tag === "Object") {
    const { _1: object } = value;
    return Object.keys(object).length;
  }
  throw new Error("Pattern match failure");
}
export { $Object as "Object" };
export { $Array as "Array" };
export { $Error as "Error" };
