import * as Foreign_Object from "../Foreign.Object/index.js";
import * as Foreign_Object_Unsafe from "../Foreign.Object.Unsafe/index.js";
export function member(key) {
  return (object) => {
    return Foreign_Object.member(key)(object);
  };
}
export function keys(object) {
  return Foreign_Object.keys(object);
}
export function index(key) {
  return (object) => {
    return Foreign_Object_Unsafe.unsafeIndex(object)(key);
  };
}
export function indexLiteral(object) {
  return Foreign_Object_Unsafe.unsafeIndex(object)("a") + Foreign_Object_Unsafe.unsafeIndex(object)("b-c") | 0;
}
