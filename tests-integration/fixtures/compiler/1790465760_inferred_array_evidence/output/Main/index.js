export const Chosen = ($value0) => ({
  tag: "Chosen",
  _1: $value0
});
export const Other = "Other";
export function test(partialDict) {
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
  const $closure$2 = (partialDict$2) => {
    const $closure$3 = ($choice$1) => {
      if ($choice$1.tag === "Chosen") {
        return 23 | 0;
      } else {
        throw new Error("Pattern match failure");
      }
    };
    return $closure$3;
  };
  return [/* @__PURE__ */ $closure(partialDict), /* @__PURE__ */ $closure$2(partialDict)];
}
function test_(partialDict) {
  const $closure = ($choice) => {
    if ($choice.tag === "Chosen") {
      const { _1: value } = $choice;
      return value;
    } else {
      throw new Error("Pattern match failure");
    }
  };
  const $closure$1 = ($choice$1) => {
    if ($choice$1.tag === "Chosen") {
      return 23 | 0;
    } else {
      throw new Error("Pattern match failure");
    }
  };
  return [$closure, $closure$1];
}
export function extract(dictionary) {
  return dictionary.extract;
}
export function test2(partialDict) {
  const $closure = (partialDict$1) => {
    const $closure$1 = ($choice) => {
      if ($choice.tag === "Chosen") {
        return 29 | 0;
      } else {
        throw new Error("Pattern match failure");
      }
    };
    return $closure$1;
  };
  return [/* @__PURE__ */ extract(extractChoice), /* @__PURE__ */ $closure(partialDict)];
}
export const extractChoice = /* @__PURE__ */ (() => {
  const $closure = ($choice) => {
    if ($choice.tag === "Chosen") {
      const { _1: value } = $choice;
      return value;
    }
    if ($choice === "Other") {
      return 41 | 0;
    }
    throw new Error("Pattern match failure");
  };
  return { extract: $closure };
})();
export { test_ as "test'" };
