export const Cons = ($value0) => ($value1) => ({
  tag: "Cons",
  _1: $value0,
  _2: $value1
});
export const Nil = "Nil";
export const Empty = "Empty";
export const Snoc = ($value0) => ($value1) => ({
  tag: "Snoc",
  _1: $value0,
  _2: $value1
});
export function first($list) {
  if ($list.tag === "Cons") {
    const { _1: value } = $list;
    return value;
  } else {
    throw new Error("Pattern match failure");
  }
}
export function second($list) {
  if ($list.tag === "Cons" && $list._2.tag === "Cons") {
    const { _1: value } = $list._2;
    return value;
  } else {
    throw new Error("Pattern match failure");
  }
}
export function inferred(partialDict) {
  return ($list) => {
    if ($list.tag === "Cons") {
      const { _1: value } = $list;
      return value;
    } else {
      throw new Error("Pattern match failure");
    }
  };
}
function second_($snoc) {
  if ($snoc.tag === "Snoc" && $snoc._1.tag === "Snoc") {
    const { _2: value } = $snoc._1;
    return value;
  } else {
    throw new Error("Pattern match failure");
  }
}
export { second_ as "second'" };
