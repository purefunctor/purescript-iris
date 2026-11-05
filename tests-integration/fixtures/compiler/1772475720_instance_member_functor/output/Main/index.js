export const Box = ($value0) => ({
  tag: "Box",
  _1: $value0
});
export function map(dictionary) {
  return dictionary.map;
}
export const functorBox = /* @__PURE__ */ (() => {
  return { map: (f) => {
    return ($box) => {
      if ($box.tag === "Box") {
        const { _1: x } = $box;
        return {
          tag: "Box",
          _1: f(x)
        };
      } else {
        throw new Error("Pattern match failure");
      }
    };
  } };
})();
