export const Box = ($value0) => ({
  tag: "Box",
  _1: $value0
});
export function show(dictionary) {
  return dictionary.show;
}
export function showBox(showADict) {
  return { show: ($box) => {
    if ($box.tag === "Box") {
      const { _1: x } = $box;
      return /* @__PURE__ */ show(showADict)(x);
    } else {
      throw new Error("Pattern match failure");
    }
  } };
}
