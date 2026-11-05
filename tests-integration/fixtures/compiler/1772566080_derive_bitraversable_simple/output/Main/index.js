import * as Control_Apply from "../Control.Apply/index.js";
import * as Data_Bitraversable from "../Data.Bitraversable/index.js";
import * as Data_Functor from "../Data.Functor/index.js";
import * as Data_Semigroup from "../Data.Semigroup/index.js";
import * as $runtime from "../runtime.js";
export const Left = ($value0) => ({
  tag: "Left",
  _1: $value0
});
export const Right = ($value0) => ({
  tag: "Right",
  _1: $value0
});
export const Pair = ($value0) => ($value1) => ({
  tag: "Pair",
  _1: $value0,
  _2: $value1
});
const $lazy_bitraversableEither = $runtime.binding("bitraversableEither", () => {
  return {
    Bifunctor0: () => bifunctorEither,
    Bifoldable1: () => bifoldableEither,
    bitraverse: (applicativeFDict) => {
      return (firstFunction) => {
        return (secondFunction) => {
          return (value) => {
            const Functor0Dict = /* @__PURE__ */ (/* @__PURE__ */ applicativeFDict.Apply0()).Functor0();
            if (value.tag === "Left") {
              const { _1: field0 } = value;
              return /* @__PURE__ */ Data_Functor.map(Functor0Dict)((field0Result) => ({
                tag: "Left",
                _1: field0Result
              }))(firstFunction(field0));
            }
            if (value.tag === "Right") {
              const { _1: field0$1 } = value;
              return /* @__PURE__ */ Data_Functor.map(Functor0Dict)((field0Result$1) => ({
                tag: "Right",
                _1: field0Result$1
              }))(secondFunction(field0$1));
            }
            throw new Error("Pattern match failure");
          };
        };
      };
    },
    bisequence: (applicativeFDict$1) => (value$1) => /* @__PURE__ */ Data_Bitraversable.bitraverse($lazy_bitraversableEither())(applicativeFDict$1)((effect0) => effect0)((effect1) => effect1)(value$1)
  };
});
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
export const bifunctorEither = /* @__PURE__ */ (() => {
  return { bimap: (firstFunction) => {
    return (secondFunction) => {
      return (value) => {
        if (value.tag === "Left") {
          const { _1: field0 } = value;
          return {
            tag: "Left",
            _1: firstFunction(field0)
          };
        }
        if (value.tag === "Right") {
          const { _1: field0$1 } = value;
          return {
            tag: "Right",
            _1: secondFunction(field0$1)
          };
        }
        throw new Error("Pattern match failure");
      };
    };
  } };
})();
export const bifoldableEither = /* @__PURE__ */ (() => {
  return {
    bifoldr: (firstFunction) => {
      return (secondFunction) => {
        return (accumulator) => {
          return (value) => {
            if (value.tag === "Left") {
              const { _1: field0 } = value;
              return firstFunction(field0)(accumulator);
            }
            if (value.tag === "Right") {
              const { _1: field0$1 } = value;
              return secondFunction(field0$1)(accumulator);
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
            if (value$1.tag === "Left") {
              const { _1: field0$2 } = value$1;
              return firstFunction$1(accumulator$1)(field0$2);
            }
            if (value$1.tag === "Right") {
              const { _1: field0$3 } = value$1;
              return secondFunction$1(accumulator$1)(field0$3);
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
            if (value$2.tag === "Left") {
              const { _1: field0$4 } = value$2;
              return firstFunction$2(field0$4);
            }
            if (value$2.tag === "Right") {
              const { _1: field0$5 } = value$2;
              return secondFunction$2(field0$5);
            }
            throw new Error("Pattern match failure");
          };
        };
      };
    }
  };
})();
export const bitraversableEither = $lazy_bitraversableEither();
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
