import * as Control_Apply from "../Control.Apply/index.js";
import * as Control_Category from "../Control.Category/index.js";
import * as Data_Bifoldable from "../Data.Bifoldable/index.js";
import * as Data_Bifunctor from "../Data.Bifunctor/index.js";
import * as Data_Bitraversable from "../Data.Bitraversable/index.js";
import * as Data_Eq from "../Data.Eq/index.js";
import * as Data_Functor from "../Data.Functor/index.js";
import * as Data_Maybe from "../Data.Maybe/index.js";
import * as Data_Monoid from "../Data.Monoid/index.js";
import * as Data_Semigroup from "../Data.Semigroup/index.js";
import * as Data_Show from "../Data.Show/index.js";
import * as $runtime from "../runtime.js";
export const Pair = ($value0) => ($value1) => ({
  tag: "Pair",
  _1: $value0,
  _2: $value1
});
export function eqPair(eqADict) {
  return (eqBDict) => {
    return { eq: (left) => {
      return (right) => {
        if (left.tag === "Pair" && right.tag === "Pair") {
          const { _1: left0, _2: left1 } = left;
          const { _1: right0, _2: right1 } = right;
          if (/* @__PURE__ */ Data_Eq.eq(eqADict)(left0)(right0)) {
            if (/* @__PURE__ */ Data_Eq.eq(eqBDict)(left1)(right1)) {
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
    } };
  };
}
const $lazy_bitraversablePair = $runtime.binding("bitraversablePair", () => {
  return {
    Bifunctor0: () => bifunctorPair,
    Bifoldable1: () => bifoldablePair,
    bitraverse: (applicativeFDict) => {
      return (firstFunction) => {
        return (secondFunction) => {
          return (value) => {
            const Apply0Dict = /* @__PURE__ */ applicativeFDict.Apply0();
            if (value.tag === "Pair") {
              const { _1: field0, _2: field1 } = value;
              return /* @__PURE__ */ Control_Apply.apply(Apply0Dict)(/* @__PURE__ */ Data_Functor.map(/* @__PURE__ */ Apply0Dict.Functor0())((field0Result) => (field1Result) => ({
                tag: "Pair",
                _1: field0Result,
                _2: field1Result
              }))(firstFunction(field0)))(secondFunction(field1));
            }
            throw new Error("Pattern match failure");
          };
        };
      };
    },
    bisequence: (applicativeFDict$1) => (value$1) => /* @__PURE__ */ Data_Bitraversable.bitraverse($lazy_bitraversablePair())(applicativeFDict$1)((effect0) => effect0)((effect1) => effect1)(value$1)
  };
});
const eqPairIntStringDict = /* @__PURE__ */ eqPair(Data_Eq.eqInt)(Data_Eq.eqString);
export const bifunctorPair = /* @__PURE__ */ (() => {
  return { bimap: (firstFunction) => {
    return (secondFunction) => {
      return (value) => {
        if (value.tag === "Pair") {
          const { _1: field0, _2: field1 } = value;
          return {
            tag: "Pair",
            _1: firstFunction(field0),
            _2: secondFunction(field1)
          };
        }
        throw new Error("Pattern match failure");
      };
    };
  } };
})();
export const bifoldablePair = /* @__PURE__ */ (() => {
  return {
    bifoldr: (firstFunction) => {
      return (secondFunction) => {
        return (accumulator) => {
          return (value) => {
            if (value.tag === "Pair") {
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
            if (value$1.tag === "Pair") {
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
            if (value$2.tag === "Pair") {
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
export const bitraversablePair = $lazy_bitraversablePair();
export const mapped = /* @__PURE__ */ Data_Eq.eq(eqPairIntStringDict)(/* @__PURE__ */ Data_Bifunctor.bimap(bifunctorPair)((section65) => section65 + (7 | 0) | 0)((section72) => section72 + "!")({
  tag: "Pair",
  _1: 3 | 0,
  _2: "right"
}))({
  tag: "Pair",
  _1: 10 | 0,
  _2: "right!"
});
export const folded = /* @__PURE__ */ Data_Bifoldable.bifoldMap(bifoldablePair)(Data_Monoid.monoidString)(/* @__PURE__ */ Data_Show.show(Data_Show.showInt))(/* @__PURE__ */ Control_Category.identity(Control_Category.categoryFn))({
  tag: "Pair",
  _1: 3 | 0,
  _2: "right"
});
export const traversed = /* @__PURE__ */ Data_Eq.eq(/* @__PURE__ */ Data_Maybe.eqMaybe(eqPairIntStringDict))(/* @__PURE__ */ Data_Bitraversable.bitraverse($lazy_bitraversablePair())(Data_Maybe.applicativeMaybe)((a) => ({
  tag: "Just",
  _1: a + (7 | 0) | 0
}))((b) => ({
  tag: "Just",
  _1: b + "!"
}))({
  tag: "Pair",
  _1: 3 | 0,
  _2: "right"
}))({
  tag: "Just",
  _1: {
    tag: "Pair",
    _1: 10 | 0,
    _2: "right!"
  }
});
