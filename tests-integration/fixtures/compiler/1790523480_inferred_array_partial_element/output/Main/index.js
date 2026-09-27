export const Chosen = ($value0) => ({
  tag: "Chosen",
  _1: $value0
});
export const Other = "Other";
export function fs(partialDict) {
  const $closure = (partialDict$1) => {
    const $closure$1 = ($choice) => {
      if ($choice.tag === "Chosen") {
        const { _1: value } = $choice;
        return value;
      } else {
        throw new Error("Pattern match failure");
      }
    };
    return $closure$1;
  };
  return [/* @__PURE__ */ $closure(partialDict)];
}
