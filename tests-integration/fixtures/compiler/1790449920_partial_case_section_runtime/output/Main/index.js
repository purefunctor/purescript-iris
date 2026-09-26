import * as Data_Functor from "../Data.Functor/index.js";
import * as Effect from "../Effect/index.js";
import * as Partial_Unsafe from "../Partial.Unsafe/index.js";
export const Blob = "Blob";
export const ArrayBuffer = "ArrayBuffer";
export function test(source) {
  const $closure = (partialDict) => {
    const $closure$1 = (section48) => {
      let $result;
      $case: {
        if (section48 === "blob") {
          $result = "Blob";
          break $case;
        }
        if (section48 === "arraybuffer") {
          $result = "ArrayBuffer";
          break $case;
        }
        throw new Error("Pattern match failure");
      }
      return /* @__PURE__ */ $result(partialDict);
    };
    return /* @__PURE__ */ Data_Functor.mapFlipped(Effect.functorEffect)(source)($closure$1);
  };
  return Partial_Unsafe.unsafePartial($closure);
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
  return Partial_Unsafe.unsafePartial((partialDict) => /* @__PURE__ */ test_(partialDict)(value));
}
export function test4(value) {
  return Partial_Unsafe.unsafePartial((partialDict) => /* @__PURE__ */ test2(partialDict)(value));
}
export function test5(source) {
  const $closure = (partialDict) => {
    const $closure$1 = ($binaryType) => {
      if ($binaryType === "Blob") {
        return 17 | 0;
      } else {
        throw new Error("Pattern match failure");
      }
    };
    return /* @__PURE__ */ Data_Functor.mapFlipped(Effect.functorEffect)(source)($closure$1);
  };
  return Partial_Unsafe.unsafePartial($closure);
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
    return /* @__PURE__ */ $closure$1(partialDict)(value);
  };
  return Partial_Unsafe.unsafePartial($closure);
}
export { test_ as "test'" };
