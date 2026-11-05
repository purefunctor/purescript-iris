import * as Data_Functor from "../Data.Functor/index.js";
import * as Effect from "../Effect/index.js";
export const Blob = "Blob";
export const ArrayBuffer = "ArrayBuffer";
export function test(source) {
  return /* @__PURE__ */ Data_Functor.mapFlipped(Effect.functorEffect)(source)((section48) => {
    return /* @__PURE__ */ ((partialDict) => {
      if (section48 === "blob") {
        return "Blob";
      }
      if (section48 === "arraybuffer") {
        return "ArrayBuffer";
      }
      throw new Error("Pattern match failure");
    })({});
  });
}
function test_(partialDict) {
  return (value) => {
    if (value === "blob") {
      return "Blob";
    }
    if (value === "arraybuffer") {
      return "ArrayBuffer";
    }
    throw new Error("Pattern match failure");
  };
}
export function test2(partialDict) {
  return ($binaryType) => {
    if ($binaryType === "Blob") {
      return 17 | 0;
    } else {
      throw new Error("Pattern match failure");
    }
  };
}
export function test3(value) {
  return /* @__PURE__ */ test_({})(value);
}
export function test4(value) {
  return /* @__PURE__ */ test2({})(value);
}
export function test5(source) {
  return /* @__PURE__ */ Data_Functor.mapFlipped(Effect.functorEffect)(source)(($binaryType) => {
    if ($binaryType === "Blob") {
      return 17 | 0;
    } else {
      throw new Error("Pattern match failure");
    }
  });
}
export function test6(value) {
  return /* @__PURE__ */ ((partialDict) => {
    return ($binaryType) => {
      if ($binaryType === "Blob") {
        return 23 | 0;
      } else {
        throw new Error("Pattern match failure");
      }
    };
  })({})(value);
}
export { test_ as "test'" };
