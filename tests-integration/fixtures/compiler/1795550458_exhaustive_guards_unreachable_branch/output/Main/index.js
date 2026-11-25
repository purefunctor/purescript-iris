export const Just = ($value0) => ({
  tag: "Just",
  _1: $value0
});
export const Nothing = "Nothing";
export function totalGuardAfterCatchAll(section25) {
  return 0 | 0;
}
export function conditionGuardAfterCatchAll(section59) {
  return 0 | 0;
}
export function refutableGuardAfterCatchAll(section91) {
  return 0 | 0;
}
export function conditionGuardAfterSamePattern(section122) {
  if (section122.tag === "Just") {
    return 0 | 0;
  }
  if (section122.tag === "Just") {
    const { _1: b } = section122;
    if (b) {
      return 1 | 0;
    }
  }
  if (section122 === "Nothing") {
    return 2 | 0;
  }
  throw new Error("Pattern match failure");
}
export function conditionGuardEquation($maybe) {
  return 0 | 0;
}
