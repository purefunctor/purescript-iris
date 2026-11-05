import * as Data_Monoid from "../Data.Monoid/index.js";
export const Identity = ($value0) => ({
  tag: "Identity",
  _1: $value0
});
export const Nothing = "Nothing";
export const Just = ($value0) => ({
  tag: "Just",
  _1: $value0
});
export const Const = ($value0) => ({
  tag: "Const",
  _1: $value0
});
export const foldableIdentity = /* @__PURE__ */ (() => {
  return {
    foldr: ($function) => {
      return (accumulator) => {
        return (value) => {
          if (value.tag === "Identity") {
            const { _1: field0 } = value;
            return $function(field0)(accumulator);
          }
          throw new Error("Pattern match failure");
        };
      };
    },
    foldl: ($function$1) => {
      return (accumulator$1) => {
        return (value$1) => {
          if (value$1.tag === "Identity") {
            const { _1: field0$1 } = value$1;
            return $function$1(accumulator$1)(field0$1);
          }
          throw new Error("Pattern match failure");
        };
      };
    },
    foldMap: (monoidMDict) => {
      return ($function$2) => {
        return (value$2) => {
          if (value$2.tag === "Identity") {
            const { _1: field0$2 } = value$2;
            return $function$2(field0$2);
          }
          throw new Error("Pattern match failure");
        };
      };
    }
  };
})();
export const foldableMaybe = /* @__PURE__ */ (() => {
  return {
    foldr: ($function) => {
      return (accumulator) => {
        return (value) => {
          if (value === "Nothing") {
            return accumulator;
          }
          if (value.tag === "Just") {
            const { _1: field0 } = value;
            return $function(field0)(accumulator);
          }
          throw new Error("Pattern match failure");
        };
      };
    },
    foldl: ($function$1) => {
      return (accumulator$1) => {
        return (value$1) => {
          if (value$1 === "Nothing") {
            return accumulator$1;
          }
          if (value$1.tag === "Just") {
            const { _1: field0$1 } = value$1;
            return $function$1(accumulator$1)(field0$1);
          }
          throw new Error("Pattern match failure");
        };
      };
    },
    foldMap: (monoidMDict) => {
      return ($function$2) => {
        return (value$2) => {
          if (value$2 === "Nothing") {
            return /* @__PURE__ */ Data_Monoid.mempty(monoidMDict);
          }
          if (value$2.tag === "Just") {
            const { _1: field0$2 } = value$2;
            return $function$2(field0$2);
          }
          throw new Error("Pattern match failure");
        };
      };
    }
  };
})();
export const foldableConstType = /* @__PURE__ */ (() => {
  return {
    foldr: ($function) => {
      return (accumulator) => {
        return (value) => {
          if (value.tag === "Const") {
            const { _1: field0 } = value;
            return accumulator;
          }
          throw new Error("Pattern match failure");
        };
      };
    },
    foldl: ($function$1) => {
      return (accumulator$1) => {
        return (value$1) => {
          if (value$1.tag === "Const") {
            const { _1: field0$1 } = value$1;
            return accumulator$1;
          }
          throw new Error("Pattern match failure");
        };
      };
    },
    foldMap: (monoidMDict) => {
      return ($function$2) => {
        return (value$2) => {
          if (value$2.tag === "Const") {
            const { _1: field0$2 } = value$2;
            return /* @__PURE__ */ Data_Monoid.mempty(monoidMDict);
          }
          throw new Error("Pattern match failure");
        };
      };
    }
  };
})();
