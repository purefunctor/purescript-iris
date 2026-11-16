import * as Data_Eq from "../Data.Eq/index.js";
import * as Data_Ord from "../Data.Ord/index.js";
export const Red = "Red";
export const Green = "Green";
export const Blue = "Blue";
export const First = "First";
export const Second = "Second";
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
const eqColourDictEq = /* @__PURE__ */ Data_Eq.eq(eqColour);
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
export const eqControl = /* @__PURE__ */ (() => {
  return { eq: (left) => {
    return (right) => {
      if (left === "First" && right === "First") {
        return true;
      }
      if (left === "Second" && right === "Second") {
        return true;
      }
      return false;
    };
  } };
})();
export const ordControl = /* @__PURE__ */ (() => {
  return {
    Eq0: () => eqControl,
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
        throw new Error("Pattern match failure");
      };
    }
  };
})();
export const equal = /* @__PURE__ */ eqColourDictEq("Red")("Red");
export const unequal = /* @__PURE__ */ eqColourDictEq("Red")("Blue");
export const ordered = /* @__PURE__ */ Data_Ord.lessThan(ordColour)("Red")("Green") && /* @__PURE__ */ Data_Ord.lessThan(ordColour)("Green")("Blue") && !/* @__PURE__ */ Data_Ord.lessThan(ordColour)("Blue")("Red");
export const control = /* @__PURE__ */ Data_Eq.notEq(eqControl)("First")("Second") && /* @__PURE__ */ Data_Ord.lessThan(ordControl)("First")("Second");
