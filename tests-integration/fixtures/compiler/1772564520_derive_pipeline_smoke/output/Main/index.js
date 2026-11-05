import * as Data_Eq from "../Data.Eq/index.js";
export const Box = "Box";
export const eqBox = /* @__PURE__ */ (() => {
  return { eq: (left) => {
    return (right) => {
      if (left === "Box" && right === "Box") {
        return true;
      }
      throw new Error("Pattern match failure");
    };
  } };
})();
export const test = /* @__PURE__ */ Data_Eq.eq(eqBox)("Box")("Box");
