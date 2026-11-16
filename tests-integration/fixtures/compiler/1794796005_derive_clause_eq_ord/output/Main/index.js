import * as Data_Eq from "../Data.Eq/index.js";
import * as Data_Ord from "../Data.Ord/index.js";
export const Red = "Red";
export const Green = "Green";
export const Blue = "Blue";
export const eqColour = /* @__PURE__ */ (() => {
  return { eq: (left) => {
    return (right) => {
      if (left === "Red" && right === "Red") {
        return true;
      }
      if (left === "Green" && right === "Green") {
        return true;
      }
      if (left === "Blue" && right === "Blue") {
        return true;
      }
      return false;
    };
  } };
})();
export const ordColour = /* @__PURE__ */ (() => {
  return {
    Eq0: () => eqColour,
    compare: (left) => {
      return (right) => {
        if (left === "Red" && right === "Red") {
          return "EQ";
        }
        if (left === "Red") {
          return "LT";
        }
        if (right === "Red") {
          return "GT";
        }
        if (left === "Green" && right === "Green") {
          return "EQ";
        }
        if (left === "Green") {
          return "LT";
        }
        if (right === "Green") {
          return "GT";
        }
        if (left === "Blue" && right === "Blue") {
          return "EQ";
        }
        throw new Error("Pattern match failure");
      };
    }
  };
})();
export const eqName = /* @__PURE__ */ (() => {
  return { eq: (left) => {
    return (right) => {
      const left0 = left;
      const right0 = right;
      if (left0 === right0) {
        return true;
      } else {
        return false;
      }
    };
  } };
})();
export const ordName = /* @__PURE__ */ (() => {
  return {
    Eq0: () => eqName,
    compare: (left) => {
      return (right) => {
        const left0 = left;
        const right0 = right;
        const $scrutinee = /* @__PURE__ */ Data_Ord.compare(Data_Ord.ordString)(left0)(right0);
        if ($scrutinee === "EQ") {
          return "EQ";
        }
        const ordering = $scrutinee;
        return ordering;
      };
    }
  };
})();
export const eqVoid = /* @__PURE__ */ (() => {
  return { eq: (left) => {
    return (right) => {
      return false;
    };
  } };
})();
export const sameColour = /* @__PURE__ */ Data_Eq.eq(eqColour)("Red")("Red") && /* @__PURE__ */ Data_Ord.lessThan(ordColour)("Red")("Green");
export const sameName = /* @__PURE__ */ Data_Eq.eq(eqName)("Alice")("Alice") && /* @__PURE__ */ Data_Ord.lessThan(ordName)("Alice")("Bob");
