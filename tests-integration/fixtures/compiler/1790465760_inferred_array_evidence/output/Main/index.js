export const Chosen = ($value0) => ({
  tag: "Chosen",
  _1: $value0
});
export const Other = "Other";
export function test(partialDict) {
  return [/* @__PURE__ */ ((partialDict$1) => {
    return ($choice) => {
      if ($choice.tag === "Chosen") {
        const { _1: value } = $choice;
        return value;
      } else {
        throw new Error("Pattern match failure");
      }
    };
  })(partialDict), /* @__PURE__ */ ((partialDict$2) => {
    return ($choice$1) => {
      if ($choice$1.tag === "Chosen") {
        return 23 | 0;
      } else {
        throw new Error("Pattern match failure");
      }
    };
  })(partialDict)];
}
function test_(partialDict) {
  return [($choice) => {
    if ($choice.tag === "Chosen") {
      const { _1: value } = $choice;
      return value;
    } else {
      throw new Error("Pattern match failure");
    }
  }, ($choice$1) => {
    if ($choice$1.tag === "Chosen") {
      return 23 | 0;
    } else {
      throw new Error("Pattern match failure");
    }
  }];
}
export function extract(dictionary) {
  return dictionary.extract;
}
export function test2(partialDict) {
  return [/* @__PURE__ */ extract(extractChoice), /* @__PURE__ */ ((partialDict$1) => {
    return ($choice) => {
      if ($choice.tag === "Chosen") {
        return 29 | 0;
      } else {
        throw new Error("Pattern match failure");
      }
    };
  })(partialDict)];
}
export const extractChoice = /* @__PURE__ */ (() => {
  return { extract: ($choice) => {
    if ($choice.tag === "Chosen") {
      const { _1: value } = $choice;
      return value;
    }
    if ($choice === "Other") {
      return 41 | 0;
    }
    throw new Error("Pattern match failure");
  } };
})();
export { test_ as "test'" };
