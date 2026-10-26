export const Pair = ($value0) => ($value1) => ({
  tag: "Pair",
  _1: $value0,
  _2: $value1
});
export function withinBinder($pair) {
  if ($pair.tag === "Pair") {
    const { _1: x, _2: x$1 } = $pair;
    return x$1;
  } else {
    throw new Error("Pattern match failure");
  }
}
