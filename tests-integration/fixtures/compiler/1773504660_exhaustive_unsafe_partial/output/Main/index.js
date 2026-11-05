export function safeZero(partialDict) {
  return ($int) => {
    if ($int === (0 | 0)) {
      return 0 | 0;
    } else {
      throw new Error("Pattern match failure");
    }
  };
}
export function unsafeZeroLambda($int) {
  if ($int === (0 | 0)) {
    return 0 | 0;
  } else {
    throw new Error("Pattern match failure");
  }
}
export const unsafeZeroName = /* @__PURE__ */ safeZero({});
