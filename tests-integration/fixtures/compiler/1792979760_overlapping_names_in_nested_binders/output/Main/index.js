export const Pair = ($value0) => ($value1) => ({
  tag: "Pair",
  _1: $value0,
  _2: $value1
});
export function acrossArguments(x) {
  return ($pair) => {
    if ($pair.tag === "Pair") {
      const { _2: x$1 } = $pair;
      return x$1;
    } else {
      throw new Error("Pattern match failure");
    }
  };
}
export function constructor($pair) {
  if ($pair.tag === "Pair") {
    const { _1: x, _2: x$1 } = $pair;
    return x$1;
  } else {
    throw new Error("Pattern match failure");
  }
}
export function operator($pair) {
  if ($pair.tag === "Pair") {
    const { _1: x, _2: x$1 } = $pair;
    return x$1;
  } else {
    throw new Error("Pattern match failure");
  }
}
export function array($array) {
  if (Array.isArray($array) && $array.length === 2) {
    const x = $array[0];
    const x$1 = $array[1];
    return x$1;
  }
  return 0 | 0;
}
export function named(x) {
  if (x.tag === "Pair") {
    const { _1: x$1 } = x;
    return 0 | 0;
  } else {
    throw new Error("Pattern match failure");
  }
}
