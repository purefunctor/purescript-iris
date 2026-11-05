import * as Control_Apply from "../Control.Apply/index.js";
import * as Data_Functor from "../Data.Functor/index.js";
import * as Data_Semigroup from "../Data.Semigroup/index.js";
import * as Data_Traversable from "../Data.Traversable/index.js";
import * as $runtime from "../runtime.js";
export const Record = ($value0) => ({
  tag: "Record",
  _1: $value0
});
const $lazy_traversableRecord = $runtime.binding("traversableRecord", () => {
  return {
    Functor0: () => functorRecord,
    Foldable1: () => foldableRecord,
    traverse: (applicativeMDict) => {
      return ($function) => {
        return (value) => {
          const Apply0Dict = /* @__PURE__ */ applicativeMDict.Apply0();
          const Functor0Dict = /* @__PURE__ */ Apply0Dict.Functor0();
          if (value.tag === "Record") {
            const { _1: field0 } = value;
            return /* @__PURE__ */ Data_Functor.map(Functor0Dict)((field0Result) => ({
              tag: "Record",
              _1: field0Result
            }))(/* @__PURE__ */ Control_Apply.apply(Apply0Dict)(/* @__PURE__ */ Data_Functor.map(Functor0Dict)((recordField0) => {
              return (recordField1) => {
                return {
                  ...field0,
                  alpha: recordField0,
                  zeta: recordField1
                };
              };
            })($function(field0.alpha)))($function(field0.zeta)));
          }
          throw new Error("Pattern match failure");
        };
      };
    },
    sequence: (applicativeMDict$1) => (value$1) => /* @__PURE__ */ Data_Traversable.traverse($lazy_traversableRecord())(applicativeMDict$1)((effect0) => effect0)(value$1)
  };
});
export const functorRecord = /* @__PURE__ */ (() => {
  return { map: ($function) => {
    return (value) => {
      if (value.tag === "Record") {
        const { _1: field0 } = value;
        return {
          tag: "Record",
          _1: {
            ...field0,
            alpha: $function(field0.alpha),
            zeta: $function(field0.zeta)
          }
        };
      }
      throw new Error("Pattern match failure");
    };
  } };
})();
export const foldableRecord = /* @__PURE__ */ (() => {
  return {
    foldr: ($function) => {
      return (accumulator) => {
        return (value) => {
          if (value.tag === "Record") {
            const { _1: field0 } = value;
            return $function(field0.alpha)($function(field0.zeta)(accumulator));
          }
          throw new Error("Pattern match failure");
        };
      };
    },
    foldl: ($function$1) => {
      return (accumulator$1) => {
        return (value$1) => {
          if (value$1.tag === "Record") {
            const { _1: field0$1 } = value$1;
            return $function$1($function$1(accumulator$1)(field0$1.alpha))(field0$1.zeta);
          }
          throw new Error("Pattern match failure");
        };
      };
    },
    foldMap: (monoidMDict) => {
      return ($function$2) => {
        return (value$2) => {
          if (value$2.tag === "Record") {
            const { _1: field0$2 } = value$2;
            return /* @__PURE__ */ Data_Semigroup.append(/* @__PURE__ */ monoidMDict.Semigroup0())($function$2(field0$2.alpha))($function$2(field0$2.zeta));
          }
          throw new Error("Pattern match failure");
        };
      };
    }
  };
})();
export const traversableRecord = $lazy_traversableRecord();
