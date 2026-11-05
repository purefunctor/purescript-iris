export const First = "First";
export const Second = "Second";
export const Third = "Third";
export const eqVoid = /* @__PURE__ */ (() => {
  return { eq: (left) => {
    return (right) => {
      return false;
    };
  } };
})();
export const ordVoid = {
  Eq0: () => eqVoid,
  compare: (left) => (right) => "EQ"
};
export const eqOrdering = /* @__PURE__ */ (() => {
  return { eq: (left) => {
    return (right) => {
      if (left === "First" && right === "First") {
        return true;
      }
      if (left === "Second" && right === "Second") {
        return true;
      }
      if (left === "Third" && right === "Third") {
        return true;
      }
      return false;
    };
  } };
})();
export const ordOrdering = /* @__PURE__ */ (() => {
  return {
    Eq0: () => eqOrdering,
    compare: (left) => {
      return (right) => {
        if (left === "First" && right === "First") {
          return "EQ";
        }
        if (left === "First") {
          return "LT";
        }
        if (right === "First") {
          return "GT";
        }
        if (left === "Second" && right === "Second") {
          return "EQ";
        }
        if (left === "Second") {
          return "LT";
        }
        if (right === "Second") {
          return "GT";
        }
        if (left === "Third" && right === "Third") {
          return "EQ";
        }
        throw new Error("Pattern match failure");
      };
    }
  };
})();
