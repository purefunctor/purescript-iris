import * as Partial_Unsafe from "../Partial.Unsafe/index.js";
export function positive(partialDict) {
  const $closure = (value) => {
    if (value > (0 | 0)) {
      return value;
    }
    throw new Error("Pattern match failure");
  };
  return $closure;
}
export function unsafePositive(value) {
  return Partial_Unsafe.unsafePartial((partialDict) => /* @__PURE__ */ positive(partialDict)(value));
}
export function unsafePositiveApplied(value) {
  return Partial_Unsafe.unsafePartial((partialDict) => /* @__PURE__ */ positive(partialDict))(value);
}
export const unsafePositiveFunction = Partial_Unsafe.unsafePartial((partialDict) => /* @__PURE__ */ positive(partialDict));
export const unsafeLambda = /* @__PURE__ */ (() => {
  const $closure = (partialDict) => {
    const $closure$1 = ($int) => {
      if ($int === (1 | 0)) {
        return 1 | 0;
      } else {
        throw new Error("Pattern match failure");
      }
    };
    return $closure$1;
  };
  return Partial_Unsafe.unsafePartial($closure);
})();
