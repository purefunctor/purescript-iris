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
  return /* @__PURE__ */ positive({})(value);
}
export function unsafePositiveApplied(value) {
  return (/* @__PURE__ */ positive({}))(value);
}
export function unsafeLambda($int) {
  if ($int === (1 | 0)) {
    return 1 | 0;
  } else {
    throw new Error("Pattern match failure");
  }
}
export const unsafePositiveFunction = /* @__PURE__ */ positive({});
