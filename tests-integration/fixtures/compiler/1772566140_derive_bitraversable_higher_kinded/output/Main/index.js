import * as Control_Apply from "../Control.Apply/index.js";
import * as Data_Bitraversable from "../Data.Bitraversable/index.js";
import * as Data_Foldable from "../Data.Foldable/index.js";
import * as Data_Functor from "../Data.Functor/index.js";
import * as Data_Semigroup from "../Data.Semigroup/index.js";
import * as Data_Traversable from "../Data.Traversable/index.js";
export const WrapBoth = ($value0) => ($value1) => ({
  tag: "WrapBoth",
  _1: $value0,
  _2: $value1
});
export function bifunctorWrapBothTypeType(functorFDict) {
  return (functorGDict) => {
    return { bimap: (firstFunction) => {
      return (secondFunction) => {
        return (value) => {
          if (value.tag === "WrapBoth") {
            const { _1: field0, _2: field1 } = value;
            return {
              tag: "WrapBoth",
              _1: /* @__PURE__ */ Data_Functor.map(functorFDict)((element) => firstFunction(element))(field0),
              _2: /* @__PURE__ */ Data_Functor.map(functorGDict)((element$1) => secondFunction(element$1))(field1)
            };
          }
          throw new Error("Pattern match failure");
        };
      };
    } };
  };
}
export function bifoldableWrapBothTypeType(foldableFDict) {
  return (foldableGDict) => {
    return {
      bifoldr: (firstFunction) => {
        return (secondFunction) => {
          return (accumulator) => {
            return (value) => {
              if (value.tag === "WrapBoth") {
                const { _1: field0, _2: field1 } = value;
                return /* @__PURE__ */ Data_Foldable.foldr(foldableFDict)((element) => (accumulator$1) => firstFunction(element)(accumulator$1))(/* @__PURE__ */ Data_Foldable.foldr(foldableGDict)((element$1) => (accumulator$2) => secondFunction(element$1)(accumulator$2))(accumulator)(field1))(field0);
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
              if (value$1.tag === "WrapBoth") {
                const { _1: field0$1, _2: field1$1 } = value$1;
                return /* @__PURE__ */ Data_Foldable.foldl(foldableGDict)((accumulator$4) => (element$2) => secondFunction$1(accumulator$4)(element$2))(/* @__PURE__ */ Data_Foldable.foldl(foldableFDict)((accumulator$5) => (element$3) => firstFunction$1(accumulator$5)(element$3))(accumulator$3)(field0$1))(field1$1);
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
              if (value$2.tag === "WrapBoth") {
                const { _1: field0$2, _2: field1$2 } = value$2;
                return /* @__PURE__ */ Data_Semigroup.append(/* @__PURE__ */ monoidMDict.Semigroup0())(/* @__PURE__ */ Data_Foldable.foldMap(foldableFDict)(monoidMDict)((element$4) => firstFunction$2(element$4))(field0$2))(/* @__PURE__ */ Data_Foldable.foldMap(foldableGDict)(monoidMDict)((element$5) => secondFunction$2(element$5))(field1$2));
              }
              throw new Error("Pattern match failure");
            };
          };
        };
      }
    };
  };
}
export function bitraversableWrapBothTypeType(traversableFDict) {
  return (traversableGDict) => {
    return {
      Bifunctor0: () => /* @__PURE__ */ bifunctorWrapBothTypeType(/* @__PURE__ */ traversableFDict.Functor0())(/* @__PURE__ */ traversableGDict.Functor0()),
      Bifoldable1: () => /* @__PURE__ */ bifoldableWrapBothTypeType(/* @__PURE__ */ traversableFDict.Foldable1())(/* @__PURE__ */ traversableGDict.Foldable1()),
      bitraverse: (applicativeFDict) => {
        return (firstFunction) => {
          return (secondFunction) => {
            return (value) => {
              const Apply0Dict = /* @__PURE__ */ applicativeFDict.Apply0();
              if (value.tag === "WrapBoth") {
                const { _1: field0, _2: field1 } = value;
                return /* @__PURE__ */ Control_Apply.apply(Apply0Dict)(/* @__PURE__ */ Data_Functor.map(/* @__PURE__ */ Apply0Dict.Functor0())((field0Result) => (field1Result) => ({
                  tag: "WrapBoth",
                  _1: field0Result,
                  _2: field1Result
                }))(/* @__PURE__ */ Data_Traversable.traverse(traversableFDict)(applicativeFDict)((element) => firstFunction(element))(field0)))(/* @__PURE__ */ Data_Traversable.traverse(traversableGDict)(applicativeFDict)((element$1) => secondFunction(element$1))(field1));
              }
              throw new Error("Pattern match failure");
            };
          };
        };
      },
      bisequence: (applicativeFDict$1) => (value$1) => /* @__PURE__ */ Data_Bitraversable.bitraverse(/* @__PURE__ */ bitraversableWrapBothTypeType(traversableFDict)(traversableGDict))(applicativeFDict$1)((effect0) => effect0)((effect1) => effect1)(value$1)
    };
  };
}
