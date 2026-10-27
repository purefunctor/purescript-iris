export function removals(values) {
  const array = values.slice();
  const last = array.length > 0 ? {
    tag: "Just",
    _1: array.pop()
  } : "Nothing";
  const first = array.length > 0 ? {
    tag: "Just",
    _1: array.shift()
  } : "Nothing";
  const empty = [];
  const emptyLast = empty.length > 0 ? {
    tag: "Just",
    _1: empty.pop()
  } : "Nothing";
  const emptyFirst = empty.length > 0 ? {
    tag: "Just",
    _1: empty.shift()
  } : "Nothing";
  const final = array.slice();
  const $value = {
    last,
    first,
    emptyLast,
    emptyFirst,
    final
  };
  return $value;
}
