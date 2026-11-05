import * as Data_Semigroup from "../Data.Semigroup/index.js";
export const Product = ($value0) => ($value1) => ($value2) => ($value3) => ({
  tag: "Product",
  _1: $value0,
  _2: $value1,
  _3: $value2,
  _4: $value3
});
export const foldableProduct = /* @__PURE__ */ (() => {
  return {
    foldr: ($function) => {
      return (accumulator) => {
        return (value) => {
          if (value.tag === "Product") {
            const { _1: field0, _2: field1, _3: field2, _4: field3 } = value;
            return $function(field0)($function(field2)($function(field3)(accumulator)));
          }
          throw new Error("Pattern match failure");
        };
      };
    },
    foldl: ($function$1) => {
      return (accumulator$1) => {
        return (value$1) => {
          if (value$1.tag === "Product") {
            const { _1: field0$1, _2: field1$1, _3: field2$1, _4: field3$1 } = value$1;
            return $function$1($function$1($function$1(accumulator$1)(field0$1))(field2$1))(field3$1);
          }
          throw new Error("Pattern match failure");
        };
      };
    },
    foldMap: (monoidMDict) => {
      return ($function$2) => {
        return (value$2) => {
          const Semigroup0Dict = /* @__PURE__ */ monoidMDict.Semigroup0();
          if (value$2.tag === "Product") {
            const { _1: field0$2, _2: field1$2, _3: field2$2, _4: field3$2 } = value$2;
            return /* @__PURE__ */ Data_Semigroup.append(Semigroup0Dict)($function$2(field0$2))(/* @__PURE__ */ Data_Semigroup.append(Semigroup0Dict)($function$2(field2$2))($function$2(field3$2)));
          }
          throw new Error("Pattern match failure");
        };
      };
    }
  };
})();
