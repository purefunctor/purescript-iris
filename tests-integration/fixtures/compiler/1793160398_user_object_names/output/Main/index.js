import * as Foreign_Object from "../Foreign.Object/index.js";
import * as Library from "../Library/index.js";
import * as $Object from "../Object/index.js";
export function keys(object) {
  return Object.keys(object);
}
export function member(key) {
  return (object) => {
    return key in object;
  };
}
export const sample = {
  tag: "Object",
  _1: Foreign_Object.insert("b")(2 | 0)(Foreign_Object.singleton("a")(1 | 0))
};
export const wrap = Library.Object;
export const marker = $Object.marker;
export { "Array", "Error", "Object", describe, objectSize } from "../Library/index.js";
