export const Just = ($value0) => ({
  tag: "Just",
  _1: $value0
});
export const Nothing = "Nothing";
export function unwrap(partialDict) {
  return (choice) => {
    if (choice.tag === "Just") {
      const { _1: value } = choice;
      return value;
    } else {
      throw new Error("Pattern match failure");
    }
  };
}
