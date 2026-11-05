export const Just = ($value0) => ({
  tag: "Just",
  _1: $value0
});
export const Nothing = "Nothing";
export const test = /* @__PURE__ */ (() => {
  return (($maybe) => {
    if ($maybe.tag === "Just") {
      return 1 | 0;
    } else {
      throw new Error("Pattern match failure");
    }
  })("Nothing");
})();
export const test2 = /* @__PURE__ */ (() => {
  return (($maybe) => {
    if ($maybe === "Nothing") {
      return 1 | 0;
    } else {
      throw new Error("Pattern match failure");
    }
  })({
    tag: "Just",
    _1: 42 | 0
  });
})();
