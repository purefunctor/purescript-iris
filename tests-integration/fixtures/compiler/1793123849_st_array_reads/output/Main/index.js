export function reads(values) {
  const array = values.slice();
  const length = array.length;
  const first = (0 | 0) >= 0 && (0 | 0) < array.length ? {
    tag: "Just",
    _1: array[0 | 0]
  } : "Nothing";
  const $index = length - (1 | 0) | 0;
  const last = $index >= 0 && $index < array.length ? {
    tag: "Just",
    _1: array[$index]
  } : "Nothing";
  const past = length >= 0 && length < array.length ? {
    tag: "Just",
    _1: array[length]
  } : "Nothing";
  const negative = (-1 | 0) >= 0 && (-1 | 0) < array.length ? {
    tag: "Just",
    _1: array[-1 | 0]
  } : "Nothing";
  const $value = {
    length,
    first,
    last,
    past,
    negative
  };
  return $value;
}
