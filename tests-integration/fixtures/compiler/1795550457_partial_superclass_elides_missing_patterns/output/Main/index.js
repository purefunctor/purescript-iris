export const Just = ($value0) => ({
  tag: "Just",
  _1: $value0
});
export const Nothing = "Nothing";
export function direct(partialDict) {
  return ($maybe) => {
    if ($maybe.tag === "Just") {
      const { _1: n } = $maybe;
      return n;
    } else {
      throw new Error("Pattern match failure");
    }
  };
}
export function viaSuperclass(unsafeDict) {
  return ($maybe) => {
    if ($maybe.tag === "Just") {
      const { _1: n } = $maybe;
      return n;
    } else {
      throw new Error("Pattern match failure");
    }
  };
}
export function viaTransitiveSuperclass(veryUnsafeDict) {
  return ($maybe) => {
    if ($maybe.tag === "Just") {
      const { _1: n } = $maybe;
      return n;
    } else {
      throw new Error("Pattern match failure");
    }
  };
}
export function withoutPartial(unrelatedDict) {
  return ($maybe) => {
    if ($maybe.tag === "Just") {
      const { _1: n } = $maybe;
      return n;
    } else {
      throw new Error("Pattern match failure");
    }
  };
}
