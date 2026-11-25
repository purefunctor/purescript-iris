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
export function operatorGuard(section32) {
  if (section32.tag === "Just") {
    const { _1: n } = section32;
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
  if (section32 === "Nothing") {
    return 0 | 0;
  }
  throw new Error("Pattern match failure");
}
export function operatorChainGuard(section73) {
  if (section73.tag === "Just") {
    const { _1: n } = section73;
    const $scrutinee = {
      tag: "Pair",
      _1: n,
      _2: {
        tag: "Pair",
        _1: n,
        _2: n
      }
    };
    if ($scrutinee.tag === "Pair" && $scrutinee._2.tag === "Pair") {
      const { _1: a } = $scrutinee;
      const { _1: b } = $scrutinee._2;
      return {
        tag: "Pair",
        _1: a,
        _2: b
      };
    }
  }
  if (section73 === "Nothing") {
    return {
      tag: "Pair",
      _1: 0 | 0,
      _2: 0 | 0
    };
  }
  throw new Error("Pattern match failure");
}
export function nestedOperatorGuard(section125) {
  if (section125.tag === "Just") {
    const { _1: n } = section125;
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
  if (section125 === "Nothing") {
    return 0 | 0;
  }
  throw new Error("Pattern match failure");
}
export function refutableOperatorGuard(partialDict) {
  return (section163) => {
    return /* @__PURE__ */ ((partialDict$1) => {
      if (section163.tag === "Just") {
        const { _1: n } = section163;
        const $scrutinee = {
          tag: "Pair",
          _1: n,
          _2: {
            tag: "Just",
            _1: n
          }
        };
        if ($scrutinee.tag === "Pair" && $scrutinee._2.tag === "Just") {
          const { _1: b } = $scrutinee._2;
          return b;
        }
      }
      if (section163 === "Nothing") {
        return 0 | 0;
      }
      throw new Error("Pattern match failure");
    })(partialDict);
  };
}
