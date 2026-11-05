import * as Data_Foldable from "../Data.Foldable/index.js";
import * as Data_Semigroup from "../Data.Semigroup/index.js";
export const Product = ($value0) => ($value1) => ({
  tag: "Product",
  _1: $value0,
  _2: $value1
});
export const RightNested = ($value0) => ({
  tag: "RightNested",
  _1: $value0
});
export const bifoldableProduct = /* @__PURE__ */ (() => {
  return {
    bifoldr: (firstFunction) => {
      return (secondFunction) => {
        return (accumulator) => {
          return (value) => {
            if (value.tag === "Product") {
              const { _1: field0, _2: field1 } = value;
              return firstFunction(field0)(secondFunction(field1)(accumulator));
            }
            throw new Error("Pattern match failure");
          };
        };
      };
    },
    bifoldl: (firstFunction$1) => {
      return (secondFunction$1) => {
        return (accumulator$1) => {
          return (value$1) => {
            if (value$1.tag === "Product") {
              const { _1: field0$1, _2: field1$1 } = value$1;
              return secondFunction$1(firstFunction$1(accumulator$1)(field0$1))(field1$1);
            }
            throw new Error("Pattern match failure");
          };
        };
      };
    },
    bifoldMap: (monoidMDict) => {
      return (firstFunction$2) => {
        return (secondFunction$2) => {
          return (value$2) => {
            if (value$2.tag === "Product") {
              const { _1: field0$2, _2: field1$2 } = value$2;
              return /* @__PURE__ */ Data_Semigroup.append(/* @__PURE__ */ monoidMDict.Semigroup0())(firstFunction$2(field0$2))(secondFunction$2(field1$2));
            }
            throw new Error("Pattern match failure");
          };
        };
      };
    }
  };
})();
export const foldableProductInt = /* @__PURE__ */ (() => {
  return {
    foldr: ($function) => {
      return (accumulator) => {
        return (value) => {
          if (value.tag === "Product") {
            const { _1: field0, _2: field1 } = value;
            return $function(field1)(accumulator);
          }
          throw new Error("Pattern match failure");
        };
      };
    },
    foldl: ($function$1) => {
      return (accumulator$1) => {
        return (value$1) => {
          if (value$1.tag === "Product") {
            const { _1: field0$1, _2: field1$1 } = value$1;
            return $function$1(accumulator$1)(field1$1);
          }
          throw new Error("Pattern match failure");
        };
      };
    },
    foldMap: (monoidMDict) => {
      return ($function$2) => {
        return (value$2) => {
          if (value$2.tag === "Product") {
            const { _1: field0$2, _2: field1$2 } = value$2;
            return $function$2(field1$2);
          }
          throw new Error("Pattern match failure");
        };
      };
    }
  };
})();
export const foldableRightNested = /* @__PURE__ */ (() => {
  return {
    foldr: ($function) => {
      return (accumulator) => {
        return (value) => {
          if (value.tag === "RightNested") {
            const { _1: field0 } = value;
            return /* @__PURE__ */ Data_Foldable.foldr(foldableProductInt)((element) => (accumulator$1) => $function(element)(accumulator$1))(accumulator)(field0);
          }
          throw new Error("Pattern match failure");
        };
      };
    },
    foldl: ($function$1) => {
      return (accumulator$2) => {
        return (value$1) => {
          if (value$1.tag === "RightNested") {
            const { _1: field0$1 } = value$1;
            return /* @__PURE__ */ Data_Foldable.foldl(foldableProductInt)((accumulator$3) => (element$1) => $function$1(accumulator$3)(element$1))(accumulator$2)(field0$1);
          }
          throw new Error("Pattern match failure");
        };
      };
    },
    foldMap: (monoidMDict) => {
      return ($function$2) => {
        return (value$2) => {
          if (value$2.tag === "RightNested") {
            const { _1: field0$2 } = value$2;
            return /* @__PURE__ */ Data_Foldable.foldMap(foldableProductInt)(monoidMDict)((element$2) => $function$2(element$2))(field0$2);
          }
          throw new Error("Pattern match failure");
        };
      };
    }
  };
})();
