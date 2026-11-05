export const Empty = "Empty";
export const One = ($value0) => ({
  tag: "One",
  _1: $value0
});
export function unwrap(wrapped) {
  const value = wrapped;
  return value;
}
export function select(record) {
  const first = record.first;
  const second = record.second;
  return second;
}
export function unwrapOne(partialDict) {
  return (choice) => {
    if (choice.tag === "One") {
      const { _1: value } = choice;
      return value;
    } else {
      throw new Error("Pattern match failure");
    }
  };
}
