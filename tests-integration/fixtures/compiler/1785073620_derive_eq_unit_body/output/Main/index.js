export const Unit = "Unit";
export const eqUnit = /* @__PURE__ */ (() => {
  return { eq: (left) => {
    return (right) => {
      if (left === "Unit" && right === "Unit") {
        return true;
      }
      throw new Error("Pattern match failure");
    };
  } };
})();
