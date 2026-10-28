import * as Data_Functor from "../Data.Functor/index.js";
import * as Effect from "../Effect/index.js";
export const Blob = "Blob";
export const ArrayBuffer = "ArrayBuffer";
export function test(source) {
  const $closure = (section48) => {
    const $closure$1 = (partialDict) => {
      if (section48 === "blob") {
        return "Blob";
      }
      if (section48 === "arraybuffer") {
        return "ArrayBuffer";
      }
      throw new Error("Pattern match failure");
    };
    return /* @__PURE__ */ $closure$1({});
  };
  return /* @__PURE__ */ Data_Functor.mapFlipped(Effect.functorEffect)(source)($closure);
}
function test_(partialDict) {
  const $closure = (value) => {
    if (value === "blob") {
      return "Blob";
    }
    if (value === "arraybuffer") {
      return "ArrayBuffer";
    }
    throw new Error("Pattern match failure");
  };
  return $closure;
}
export function test2(partialDict) {
  const $closure = ($binaryType) => {
    if ($binaryType === "Blob") {
      return 17 | 0;
    } else {
      throw new Error("Pattern match failure");
    }
  };
  return $closure;
}
export function test3(value) {
  return /* @__PURE__ */ test_({})(value);
}
export function test4(value) {
  return /* @__PURE__ */ test2({})(value);
}
export function test5(source) {
  const $closure = ($binaryType) => {
    if ($binaryType === "Blob") {
      return 17 | 0;
    } else {
      throw new Error("Pattern match failure");
    }
  };
  return /* @__PURE__ */ Data_Functor.mapFlipped(Effect.functorEffect)(source)($closure);
}
export function test6(value) {
  const $closure = (partialDict) => {
    const $closure$1 = ($binaryType) => {
      if ($binaryType === "Blob") {
        return 23 | 0;
      } else {
        throw new Error("Pattern match failure");
      }
    };
    return $closure$1;
  };
  return /* @__PURE__ */ $closure({})(value);
}
export { test_ as "test'" };
