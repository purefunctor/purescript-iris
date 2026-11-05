import * as Control_Apply from "../Control.Apply/index.js";
import * as Data_Bitraversable from "../Data.Bitraversable/index.js";
import * as Data_Functor from "../Data.Functor/index.js";
import * as Data_Semigroup from "../Data.Semigroup/index.js";
import * as $runtime from "../runtime.js";
export const RecordPair = ($value0) => ({
  tag: "RecordPair",
  _1: $value0
});
const $lazy_bitraversableRecordPair = $runtime.binding("bitraversableRecordPair", () => {
  return {
    Bifunctor0: () => bifunctorRecordPair,
    Bifoldable1: () => bifoldableRecordPair,
    bitraverse: (applicativeFDict) => {
      return (firstFunction) => {
        return (secondFunction) => {
          return (value) => {
            const Apply0Dict = /* @__PURE__ */ applicativeFDict.Apply0();
            const Functor0Dict = /* @__PURE__ */ Apply0Dict.Functor0();
            if (value.tag === "RecordPair") {
              const { _1: field0 } = value;
              return /* @__PURE__ */ Data_Functor.map(Functor0Dict)((field0Result) => ({
                tag: "RecordPair",
                _1: field0Result
              }))(/* @__PURE__ */ Control_Apply.apply(Apply0Dict)(/* @__PURE__ */ Data_Functor.map(Functor0Dict)((recordField0) => {
                return (recordField1) => {
                  return {
                    ...field0,
                    alpha: recordField0,
                    zeta: recordField1
                  };
                };
              })(firstFunction(field0.alpha)))(secondFunction(field0.zeta)));
            }
            throw new Error("Pattern match failure");
          };
        };
      };
    },
    bisequence: (applicativeFDict$1) => (value$1) => /* @__PURE__ */ Data_Bitraversable.bitraverse($lazy_bitraversableRecordPair())(applicativeFDict$1)((effect0) => effect0)((effect1) => effect1)(value$1)
  };
});
export const bifunctorRecordPair = /* @__PURE__ */ (() => {
  return { bimap: (firstFunction) => {
    return (secondFunction) => {
      return (value) => {
        if (value.tag === "RecordPair") {
          const { _1: field0 } = value;
          return {
            tag: "RecordPair",
            _1: {
              ...field0,
              alpha: firstFunction(field0.alpha),
              zeta: secondFunction(field0.zeta)
            }
          };
        }
        throw new Error("Pattern match failure");
      };
    };
  } };
})();
export const bifoldableRecordPair = /* @__PURE__ */ (() => {
  return {
    bifoldr: (firstFunction) => {
      return (secondFunction) => {
        return (accumulator) => {
          return (value) => {
            if (value.tag === "RecordPair") {
              const { _1: field0 } = value;
              return firstFunction(field0.alpha)(secondFunction(field0.zeta)(accumulator));
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
            if (value$1.tag === "RecordPair") {
              const { _1: field0$1 } = value$1;
              return secondFunction$1(firstFunction$1(accumulator$1)(field0$1.alpha))(field0$1.zeta);
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
            if (value$2.tag === "RecordPair") {
              const { _1: field0$2 } = value$2;
              return /* @__PURE__ */ Data_Semigroup.append(/* @__PURE__ */ monoidMDict.Semigroup0())(firstFunction$2(field0$2.alpha))(secondFunction$2(field0$2.zeta));
            }
            throw new Error("Pattern match failure");
          };
        };
      };
    }
  };
})();
export const bitraversableRecordPair = $lazy_bitraversableRecordPair();
