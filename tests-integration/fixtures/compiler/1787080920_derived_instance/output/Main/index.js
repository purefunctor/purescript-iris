import * as Data_Eq from "../Data.Eq/index.js";
import * as Data_Show from "../Data.Show/index.js";
export const Box = "Box";
export function showIdentity(showADict) {
  return showADict;
}
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
export const equal = /* @__PURE__ */ Data_Eq.eq(eqBox)("Box")("Box");
export const rendered = /* @__PURE__ */ Data_Show.show(/* @__PURE__ */ showIdentity(Data_Show.showInt))(42 | 0);
