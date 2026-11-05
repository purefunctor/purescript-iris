import * as Data_Semigroup from "../Data.Semigroup/index.js";
export const Record = ($value0) => ({
  tag: "Record",
  _1: $value0
});
export const foldableRecord = /* @__PURE__ */ (() => {
  return {
    foldr: ($function) => {
      return (accumulator) => {
        return (value) => {
          if (value.tag === "Record") {
            const { _1: field0 } = value;
            return $function(field0.alpha)($function(field0.zeta)(accumulator));
          }
          throw new Error("Pattern match failure");
        };
      };
    },
    foldl: ($function$1) => {
      return (accumulator$1) => {
        return (value$1) => {
          if (value$1.tag === "Record") {
            const { _1: field0$1 } = value$1;
            return $function$1($function$1(accumulator$1)(field0$1.alpha))(field0$1.zeta);
          }
          throw new Error("Pattern match failure");
        };
      };
    },
    foldMap: (monoidMDict) => {
      return ($function$2) => {
        return (value$2) => {
          if (value$2.tag === "Record") {
            const { _1: field0$2 } = value$2;
            return /* @__PURE__ */ Data_Semigroup.append(/* @__PURE__ */ monoidMDict.Semigroup0())($function$2(field0$2.alpha))($function$2(field0$2.zeta));
          }
          throw new Error("Pattern match failure");
        };
      };
    }
  };
})();
