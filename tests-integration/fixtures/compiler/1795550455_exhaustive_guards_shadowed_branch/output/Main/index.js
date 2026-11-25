export const Just = ($value0) => ({
  tag: "Just",
  _1: $value0
});
export const Nothing = "Nothing";
export function shadowedBranch(section25) {
  if (section25.tag === "Just") {
    const { _1: n } = section25;
    const m = n;
    return m;
  }
  if (section25.tag === "Just") {
    const { _1: n$1 } = section25;
    return n$1;
  }
  if (section25 === "Nothing") {
    return 0 | 0;
  }
  throw new Error("Pattern match failure");
}
