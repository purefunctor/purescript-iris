import * as Data_Boolean from "../Data.Boolean/index.js";
export const Just = ($value0) => ({
  tag: "Just",
  _1: $value0
});
export const Nothing = "Nothing";
export function test1(partialDict) {
  return (section21) => {
    return /* @__PURE__ */ ((partialDict$1) => {
      if (section21.tag === "Just") {
        if (Data_Boolean.otherwise) {
          return 1 | 0;
        }
      }
      throw new Error("Pattern match failure");
    })(partialDict);
  };
}
export function test2(partialDict) {
  return (section38) => {
    return /* @__PURE__ */ ((partialDict$1) => {
      if (section38 === "Nothing") {
        if (true) {
          return 2 | 0;
        }
      }
      throw new Error("Pattern match failure");
    })(partialDict);
  };
}
