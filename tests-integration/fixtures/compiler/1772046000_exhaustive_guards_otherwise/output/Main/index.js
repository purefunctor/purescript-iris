import * as Data_Boolean from "../Data.Boolean/index.js";
export const Just = ($value0) => ({
  tag: "Just",
  _1: $value0
});
export const Nothing = "Nothing";
export function test1(partialDict) {
  const $closure = (section21) => {
    const $closure$1 = (partialDict$1) => {
      if (section21.tag === "Just") {
        if (Data_Boolean.otherwise) {
          return 1 | 0;
        }
      }
      throw new Error("Pattern match failure");
    };
    return /* @__PURE__ */ $closure$1(partialDict);
  };
  return $closure;
}
export function test2(partialDict) {
  const $closure = (section38) => {
    const $closure$1 = (partialDict$1) => {
      if (section38 === "Nothing") {
        if (true) {
          return 2 | 0;
        }
      }
      throw new Error("Pattern match failure");
    };
    return /* @__PURE__ */ $closure$1(partialDict);
  };
  return $closure;
}
