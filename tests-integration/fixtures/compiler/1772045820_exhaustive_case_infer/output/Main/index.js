export const Just = ($value0) => ({
  tag: "Just",
  _1: $value0
});
export const Nothing = "Nothing";
export function test(partialDict) {
  const $closure = (section15) => {
    const $closure$1 = (partialDict$1) => {
      if (section15.tag === "Just") {
        return 1 | 0;
      }
      throw new Error("Pattern match failure");
    };
    return /* @__PURE__ */ $closure$1(partialDict);
  };
  return $closure;
}
