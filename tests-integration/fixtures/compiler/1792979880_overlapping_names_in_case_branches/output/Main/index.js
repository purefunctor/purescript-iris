export const Pair = ($value0) => ($value1) => ({
  tag: "Pair",
  _1: $value0,
  _2: $value1
});
export function multipleScrutinees(a) {
  return (b) => {
    const x = a;
    const x$1 = b;
    return x$1;
  };
}
export function singleScrutinee(p) {
  if (p.tag === "Pair") {
    const { _1: x, _2: x$1 } = p;
    return x$1;
  }
  throw new Error("Pattern match failure");
}
export function separateBranches(a) {
  return (b) => {
    if (a === (0 | 0)) {
      const x = b;
      return x;
    }
    const x$1 = a;
    return x$1;
  };
}
