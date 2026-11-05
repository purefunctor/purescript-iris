import * as Control_Apply from "../Control.Apply/index.js";
import * as Data_Functor from "../Data.Functor/index.js";
import * as Data_Semigroup from "../Data.Semigroup/index.js";
import * as Data_Traversable from "../Data.Traversable/index.js";
import * as $runtime from "../runtime.js";
export const Product = ($value0) => ($value1) => ($value2) => ({
  tag: "Product",
  _1: $value0,
  _2: $value1,
  _3: $value2
});
const $lazy_traversableProduct = $runtime.binding("traversableProduct", () => {
  return {
    Functor0: () => functorProduct,
    Foldable1: () => foldableProduct,
    traverse: (applicativeMDict) => {
      return ($function) => {
        return (value) => {
          const Apply0Dict = /* @__PURE__ */ applicativeMDict.Apply0();
          if (value.tag === "Product") {
            const { _1: field0, _2: field1, _3: field2 } = value;
            return /* @__PURE__ */ Control_Apply.apply(Apply0Dict)(/* @__PURE__ */ Data_Functor.map(/* @__PURE__ */ Apply0Dict.Functor0())((field0Result) => (field2Result) => ({
              tag: "Product",
              _1: field0Result,
              _2: field1,
              _3: field2Result
            }))($function(field0)))($function(field2));
          }
          throw new Error("Pattern match failure");
        };
      };
    },
    sequence: (applicativeMDict$1) => (value$1) => /* @__PURE__ */ Data_Traversable.traverse($lazy_traversableProduct())(applicativeMDict$1)((effect0) => effect0)(value$1)
  };
});
export const functorProduct = /* @__PURE__ */ (() => {
  return { map: ($function) => {
    return (value) => {
      if (value.tag === "Product") {
        const { _1: field0, _2: field1, _3: field2 } = value;
        return {
          tag: "Product",
          _1: $function(field0),
          _2: field1,
          _3: $function(field2)
        };
      }
      throw new Error("Pattern match failure");
    };
  } };
})();
export const foldableProduct = /* @__PURE__ */ (() => {
  return {
    foldr: ($function) => {
      return (accumulator) => {
        return (value) => {
          if (value.tag === "Product") {
            const { _1: field0, _2: field1, _3: field2 } = value;
            return $function(field0)($function(field2)(accumulator));
          }
          throw new Error("Pattern match failure");
        };
      };
    },
    foldl: ($function$1) => {
      return (accumulator$1) => {
        return (value$1) => {
          if (value$1.tag === "Product") {
            const { _1: field0$1, _2: field1$1, _3: field2$1 } = value$1;
            return $function$1($function$1(accumulator$1)(field0$1))(field2$1);
          }
          throw new Error("Pattern match failure");
        };
      };
    },
    foldMap: (monoidMDict) => {
      return ($function$2) => {
        return (value$2) => {
          if (value$2.tag === "Product") {
            const { _1: field0$2, _2: field1$2, _3: field2$2 } = value$2;
            return /* @__PURE__ */ Data_Semigroup.append(/* @__PURE__ */ monoidMDict.Semigroup0())($function$2(field0$2))($function$2(field2$2));
          }
          throw new Error("Pattern match failure");
        };
      };
    }
  };
})();
export const traversableProduct = $lazy_traversableProduct();
