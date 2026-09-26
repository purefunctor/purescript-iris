import * as Data_Eq from "../Data.Eq/index.js";
export const Hours = "Hours";
export const Minutes = "Minutes";
export const Seconds = "Seconds";
export const Duration = ($value0) => ($value1) => ({
  tag: "Duration",
  _1: $value0,
  _2: $value1
});
export const eqDurationComponent = /* @__PURE__ */ (() => {
  const $closure = (left) => {
    return (right) => {
      if (left === "Hours" && right === "Hours") {
        return true;
      }
      if (left === "Minutes" && right === "Minutes") {
        return true;
      }
      if (left === "Seconds" && right === "Seconds") {
        return true;
      }
      return false;
    };
  };
  return { eq: $closure };
})();
export const eqDuration = /* @__PURE__ */ (() => {
  const $closure = (left) => {
    return (right) => {
      if (left.tag === "Duration" && right.tag === "Duration") {
        const { _1: left0, _2: left1 } = left;
        const { _1: right0, _2: right1 } = right;
        if (/* @__PURE__ */ Data_Eq.eq(eqDurationComponent)(left0)(right0)) {
          if (/* @__PURE__ */ Data_Eq.eq(Data_Eq.eqInt)(left1)(right1)) {
            return true;
          } else {
            return false;
          }
        } else {
          return false;
        }
      }
      throw new Error("Pattern match failure");
    };
  };
  return { eq: $closure };
})();
