export const Just = ($value0) => ({
  tag: "Just",
  _1: $value0
});
export const Nothing = "Nothing";
export const Pair = ($value0) => ($value1) => ({
  tag: "Pair",
  _1: $value0,
  _2: $value1
});
export function newtypeGuard(section31) {
  if (section31.tag === "Just") {
    const { _1: n } = section31;
    const m = n;
    return m;
  }
  if (section31 === "Nothing") {
    return 0 | 0;
  }
  throw new Error("Pattern match failure");
}
export function singleConstructorGuard(section65) {
  if (section65.tag === "Just") {
    const { _1: n } = section65;
    const $scrutinee = {
      tag: "Pair",
      _1: n,
      _2: n
    };
    if ($scrutinee.tag === "Pair") {
      const { _1: a } = $scrutinee;
      return a;
    }
  }
  if (section65 === "Nothing") {
    return 0 | 0;
  }
  throw new Error("Pattern match failure");
}
export function recordGuard(section102) {
  if (section102.tag === "Just") {
    const { _1: n } = section102;
    const $scrutinee = { value: n };
    const value = $scrutinee.value;
    return value;
  }
  if (section102 === "Nothing") {
    return 0 | 0;
  }
  throw new Error("Pattern match failure");
}
export function nestedGuard(section138) {
  if (section138.tag === "Just") {
    const { _1: n } = section138;
    const $scrutinee = {
      tag: "Pair",
      _1: n,
      _2: { value: n }
    };
    if ($scrutinee.tag === "Pair") {
      const { _1: a } = $scrutinee;
      const value = $scrutinee._2.value;
      return {
        tag: "Pair",
        _1: a,
        _2: value
      };
    }
  }
  if (section138 === "Nothing") {
    return {
      tag: "Pair",
      _1: 0 | 0,
      _2: 0 | 0
    };
  }
  throw new Error("Pattern match failure");
}
export function refutableGuard(partialDict) {
  return (section188) => {
    return /* @__PURE__ */ ((partialDict$1) => {
      if (section188.tag === "Just") {
        const { _1: n } = section188;
        const $scrutinee = {
          tag: "Just",
          _1: n
        };
        if ($scrutinee.tag === "Just") {
          const { _1: m } = $scrutinee;
          return m;
        }
      }
      if (section188 === "Nothing") {
        return 0 | 0;
      }
      throw new Error("Pattern match failure");
    })(partialDict);
  };
}
