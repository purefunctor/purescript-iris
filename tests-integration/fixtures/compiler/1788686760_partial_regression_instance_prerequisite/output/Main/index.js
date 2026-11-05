export const Wrapped = ($value0) => ({
  tag: "Wrapped",
  _1: $value0
});
export function select(dictionary) {
  return dictionary.select;
}
export function selectWrapped(selectADict) {
  return { select: ($wrapped) => {
    if ($wrapped.tag === "Wrapped") {
      const { _1: first } = $wrapped;
      return ($wrapped$1) => {
        if ($wrapped$1.tag === "Wrapped") {
          const { _1: second } = $wrapped$1;
          return {
            tag: "Wrapped",
            _1: /* @__PURE__ */ select(selectADict)(first)(second)
          };
        } else {
          throw new Error("Pattern match failure");
        }
      };
    } else {
      throw new Error("Pattern match failure");
    }
  } };
}
export const selectInt = { select: (first) => (second) => second };
export const ordinaryPrerequisite = /* @__PURE__ */ select(/* @__PURE__ */ selectWrapped(selectInt))({
  tag: "Wrapped",
  _1: 10 | 0
})({
  tag: "Wrapped",
  _1: 20 | 0
});
