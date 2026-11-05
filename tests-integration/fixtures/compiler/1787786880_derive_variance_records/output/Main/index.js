import * as Data_Functor from "../Data.Functor/index.js";
import * as Data_Traversable from "../Data.Traversable/index.js";
import * as $runtime from "../runtime.js";
export const CovariantRecord = ($value0) => ({
  tag: "CovariantRecord",
  _1: $value0
});
export const BivariantRecord = ($value0) => ({
  tag: "BivariantRecord",
  _1: $value0
});
export const ContravariantRecord = ($value0) => ({
  tag: "ContravariantRecord",
  _1: $value0
});
export const ProfunctorRecord = ($value0) => ({
  tag: "ProfunctorRecord",
  _1: $value0
});
const $lazy_traversableCovariantRecord = $runtime.binding("traversableCovariantRecord", () => {
  return {
    Functor0: () => functorCovariantRecord,
    Foldable1: () => foldableCovariantRecord,
    traverse: (applicativeMDict) => {
      return ($function) => {
        return (value) => {
          const Functor0Dict = /* @__PURE__ */ (/* @__PURE__ */ applicativeMDict.Apply0()).Functor0();
          if (value.tag === "CovariantRecord") {
            const { _1: field0 } = value;
            return /* @__PURE__ */ Data_Functor.map(Functor0Dict)((field0Result) => ({
              tag: "CovariantRecord",
              _1: field0Result
            }))(/* @__PURE__ */ Data_Functor.map(Functor0Dict)((recordField0) => {
              return {
                ...field0,
                value: recordField0
              };
            })($function(field0.value)));
          }
          throw new Error("Pattern match failure");
        };
      };
    },
    sequence: (applicativeMDict$1) => (value$1) => /* @__PURE__ */ Data_Traversable.traverse($lazy_traversableCovariantRecord())(applicativeMDict$1)((effect0) => effect0)(value$1)
  };
});
export const functorCovariantRecord = /* @__PURE__ */ (() => {
  return { map: ($function) => {
    return (value) => {
      if (value.tag === "CovariantRecord") {
        const { _1: field0 } = value;
        return {
          tag: "CovariantRecord",
          _1: {
            ...field0,
            value: $function(field0.value)
          }
        };
      }
      throw new Error("Pattern match failure");
    };
  } };
})();
export const foldableCovariantRecord = /* @__PURE__ */ (() => {
  return {
    foldr: ($function) => {
      return (accumulator) => {
        return (value) => {
          if (value.tag === "CovariantRecord") {
            const { _1: field0 } = value;
            return $function(field0.value)(accumulator);
          }
          throw new Error("Pattern match failure");
        };
      };
    },
    foldl: ($function$1) => {
      return (accumulator$1) => {
        return (value$1) => {
          if (value$1.tag === "CovariantRecord") {
            const { _1: field0$1 } = value$1;
            return $function$1(accumulator$1)(field0$1.value);
          }
          throw new Error("Pattern match failure");
        };
      };
    },
    foldMap: (monoidMDict) => {
      return ($function$2) => {
        return (value$2) => {
          if (value$2.tag === "CovariantRecord") {
            const { _1: field0$2 } = value$2;
            return $function$2(field0$2.value);
          }
          throw new Error("Pattern match failure");
        };
      };
    }
  };
})();
export const traversableCovariantRecord = $lazy_traversableCovariantRecord();
export const bifunctorBivariantRecord = /* @__PURE__ */ (() => {
  return { bimap: (firstFunction) => {
    return (secondFunction) => {
      return (value) => {
        if (value.tag === "BivariantRecord") {
          const { _1: field0 } = value;
          return {
            tag: "BivariantRecord",
            _1: {
              ...field0,
              first: firstFunction(field0.first),
              second: secondFunction(field0.second)
            }
          };
        }
        throw new Error("Pattern match failure");
      };
    };
  } };
})();
export const contravariantContravariantRecord = /* @__PURE__ */ (() => {
  return { cmap: ($function) => {
    return (value) => {
      if (value.tag === "ContravariantRecord") {
        const { _1: field0 } = value;
        return {
          tag: "ContravariantRecord",
          _1: {
            ...field0,
            predicate: (argument) => field0.predicate($function(argument))
          }
        };
      }
      throw new Error("Pattern match failure");
    };
  } };
})();
export const profunctorProfunctorRecord = /* @__PURE__ */ (() => {
  return { dimap: (firstFunction) => {
    return (secondFunction) => {
      return (value) => {
        if (value.tag === "ProfunctorRecord") {
          const { _1: field0 } = value;
          return {
            tag: "ProfunctorRecord",
            _1: {
              ...field0,
              run: (argument) => secondFunction(field0.run(firstFunction(argument)))
            }
          };
        }
        throw new Error("Pattern match failure");
      };
    };
  } };
})();
