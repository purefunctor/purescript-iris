export const Just = ($value0) => ({
  tag: "Just",
  _1: $value0
});
export const Nothing = "Nothing";
export function test(partialDict) {
  return (section15) => {
    return /* @__PURE__ */ ((partialDict$1) => {
      if (section15.tag === "Just") {
        return 1 | 0;
      }
      throw new Error("Pattern match failure");
    })(partialDict);
  };
}
