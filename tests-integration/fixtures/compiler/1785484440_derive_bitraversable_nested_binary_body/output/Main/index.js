import * as Data_Bifoldable from "../Data.Bifoldable/index.js";
import * as Data_Bifunctor from "../Data.Bifunctor/index.js";
import * as Data_Bitraversable from "../Data.Bitraversable/index.js";
import * as Data_Functor from "../Data.Functor/index.js";
export const Nested = ($value0) => ({
  tag: "Nested",
  _1: $value0
});
export function bifunctorNestedTypeType(bifunctorPDict) {
  return { bimap: (firstFunction) => {
    return (secondFunction) => {
      return (value) => {
        if (value.tag === "Nested") {
          const { _1: field0 } = value;
          return {
            tag: "Nested",
            _1: /* @__PURE__ */ Data_Bifunctor.bimap(bifunctorPDict)((element) => firstFunction(element))((element$1) => secondFunction(element$1))(field0)
          };
        }
        throw new Error("Pattern match failure");
      };
    };
  } };
}
export function bifoldableNestedTypeType(bifoldablePDict) {
  return {
    bifoldr: (firstFunction) => {
      return (secondFunction) => {
        return (accumulator) => {
          return (value) => {
            if (value.tag === "Nested") {
              const { _1: field0 } = value;
              return /* @__PURE__ */ Data_Bifoldable.bifoldr(bifoldablePDict)((element) => (accumulator$1) => firstFunction(element)(accumulator$1))((element$1) => (accumulator$2) => secondFunction(element$1)(accumulator$2))(accumulator)(field0);
            }
            throw new Error("Pattern match failure");
          };
        };
      };
    },
    bifoldl: (firstFunction$1) => {
      return (secondFunction$1) => {
        return (accumulator$3) => {
          return (value$1) => {
            if (value$1.tag === "Nested") {
              const { _1: field0$1 } = value$1;
              return /* @__PURE__ */ Data_Bifoldable.bifoldl(bifoldablePDict)((accumulator$4) => (element$2) => firstFunction$1(accumulator$4)(element$2))((accumulator$5) => (element$3) => secondFunction$1(accumulator$5)(element$3))(accumulator$3)(field0$1);
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
            if (value$2.tag === "Nested") {
              const { _1: field0$2 } = value$2;
              return /* @__PURE__ */ Data_Bifoldable.bifoldMap(bifoldablePDict)(monoidMDict)((element$4) => firstFunction$2(element$4))((element$5) => secondFunction$2(element$5))(field0$2);
            }
            throw new Error("Pattern match failure");
          };
        };
      };
    }
  };
}
export function bitraversableNestedTypeType(bitraversablePDict) {
  return {
    Bifunctor0: () => /* @__PURE__ */ bifunctorNestedTypeType(/* @__PURE__ */ bitraversablePDict.Bifunctor0()),
    Bifoldable1: () => /* @__PURE__ */ bifoldableNestedTypeType(/* @__PURE__ */ bitraversablePDict.Bifoldable1()),
    bitraverse: (applicativeFDict) => {
      return (firstFunction) => {
        return (secondFunction) => {
          return (value) => {
            if (value.tag === "Nested") {
              const { _1: field0 } = value;
              return /* @__PURE__ */ Data_Functor.map(/* @__PURE__ */ (/* @__PURE__ */ applicativeFDict.Apply0()).Functor0())((field0Result) => ({
                tag: "Nested",
                _1: field0Result
              }))(/* @__PURE__ */ Data_Bitraversable.bitraverse(bitraversablePDict)(applicativeFDict)((element) => firstFunction(element))((element$1) => secondFunction(element$1))(field0));
            }
            throw new Error("Pattern match failure");
          };
        };
      };
    },
    bisequence: (applicativeFDict$1) => (value$1) => /* @__PURE__ */ Data_Bitraversable.bitraverse(/* @__PURE__ */ bitraversableNestedTypeType(bitraversablePDict))(applicativeFDict$1)((effect0) => effect0)((effect1) => effect1)(value$1)
  };
}
