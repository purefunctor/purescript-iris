export const Empty = "Empty";
export const One = ($value0) => ({
  tag: "One",
  _1: $value0
});
export const Pair = ($value0) => ($value1) => ({
  tag: "Pair",
  _1: $value0,
  _2: $value1
});
export function checked(choice) {
  if (choice === "Empty") {
    return 0 | 0;
  }
  if (choice.tag === "One") {
    const { _1: value } = choice;
    return value;
  }
  if (choice.tag === "Pair") {
    const { _1: left } = choice;
    return left;
  }
  throw new Error("Pattern match failure");
}
export function inferred(choice) {
  if (choice === "Empty") {
    return 0 | 0;
  }
  if (choice.tag === "One") {
    const { _1: value } = choice;
    return value;
  }
  if (choice.tag === "Pair") {
    const { _1: left } = choice;
    return left;
  }
  throw new Error("Pattern match failure");
}
export function multiple(first) {
  return (second) => {
    if (first.tag === "One" && second.tag === "One") {
      const { _1: left } = first;
      return left;
    }
    if (first.tag === "Pair") {
      const { _1: left$1 } = first;
      return left$1;
    }
    if (second.tag === "One") {
      const { _1: right } = second;
      return right;
    }
    return 0 | 0;
  };
}
export function partial(partialDict) {
  return (choice) => {
    if (choice.tag === "One") {
      const { _1: value } = choice;
      return value;
    }
    throw new Error("Pattern match failure");
  };
}
