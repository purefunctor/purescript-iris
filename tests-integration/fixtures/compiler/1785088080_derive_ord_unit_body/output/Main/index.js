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
export const ordUnit = /* @__PURE__ */ (() => {
  return {
    Eq0: () => eqUnit,
    compare: (left) => {
      return (right) => {
        if (left === "Unit" && right === "Unit") {
          return "EQ";
        }
        throw new Error("Pattern match failure");
      };
    }
  };
})();
