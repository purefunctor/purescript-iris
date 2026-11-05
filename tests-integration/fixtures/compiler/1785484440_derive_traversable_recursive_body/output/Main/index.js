import * as Control_Apply from "../Control.Apply/index.js";
import * as Data_Foldable from "../Data.Foldable/index.js";
import * as Data_Functor from "../Data.Functor/index.js";
import * as Data_Semigroup from "../Data.Semigroup/index.js";
import * as Data_Traversable from "../Data.Traversable/index.js";
import * as $runtime from "../runtime.js";
export const Leaf = ($value0) => ({
  tag: "Leaf",
  _1: $value0
});
export const Branch = ($value0) => ($value1) => ({
  tag: "Branch",
  _1: $value0,
  _2: $value1
});
const $lazy_functorTree = $runtime.binding("functorTree", () => {
  return { map: ($function) => {
    return (value) => {
      if (value.tag === "Leaf") {
        const { _1: field0 } = value;
        return {
          tag: "Leaf",
          _1: $function(field0)
        };
      }
      if (value.tag === "Branch") {
        const { _1: field0$1, _2: field1 } = value;
        return {
          tag: "Branch",
          _1: /* @__PURE__ */ Data_Functor.map($lazy_functorTree())((element) => $function(element))(field0$1),
          _2: /* @__PURE__ */ Data_Functor.map($lazy_functorTree())((element$1) => $function(element$1))(field1)
        };
      }
      throw new Error("Pattern match failure");
    };
  } };
});
const $lazy_foldableTree = $runtime.binding("foldableTree", () => {
  return {
    foldr: ($function) => {
      return (accumulator) => {
        return (value) => {
          if (value.tag === "Leaf") {
            const { _1: field0 } = value;
            return $function(field0)(accumulator);
          }
          if (value.tag === "Branch") {
            const { _1: field0$1, _2: field1 } = value;
            return /* @__PURE__ */ Data_Foldable.foldr($lazy_foldableTree())((element) => (accumulator$1) => $function(element)(accumulator$1))(/* @__PURE__ */ Data_Foldable.foldr($lazy_foldableTree())((element$1) => (accumulator$2) => $function(element$1)(accumulator$2))(accumulator)(field1))(field0$1);
          }
          throw new Error("Pattern match failure");
        };
      };
    },
    foldl: ($function$1) => {
      return (accumulator$3) => {
        return (value$1) => {
          if (value$1.tag === "Leaf") {
            const { _1: field0$2 } = value$1;
            return $function$1(accumulator$3)(field0$2);
          }
          if (value$1.tag === "Branch") {
            const { _1: field0$3, _2: field1$1 } = value$1;
            return /* @__PURE__ */ Data_Foldable.foldl($lazy_foldableTree())((accumulator$4) => (element$2) => $function$1(accumulator$4)(element$2))(/* @__PURE__ */ Data_Foldable.foldl($lazy_foldableTree())((accumulator$5) => (element$3) => $function$1(accumulator$5)(element$3))(accumulator$3)(field0$3))(field1$1);
          }
          throw new Error("Pattern match failure");
        };
      };
    },
    foldMap: (monoidMDict) => {
      return ($function$2) => {
        return (value$2) => {
          if (value$2.tag === "Leaf") {
            const { _1: field0$4 } = value$2;
            return $function$2(field0$4);
          }
          if (value$2.tag === "Branch") {
            const { _1: field0$5, _2: field1$2 } = value$2;
            return /* @__PURE__ */ Data_Semigroup.append(/* @__PURE__ */ monoidMDict.Semigroup0())(/* @__PURE__ */ Data_Foldable.foldMap($lazy_foldableTree())(monoidMDict)((element$4) => $function$2(element$4))(field0$5))(/* @__PURE__ */ Data_Foldable.foldMap($lazy_foldableTree())(monoidMDict)((element$5) => $function$2(element$5))(field1$2));
          }
          throw new Error("Pattern match failure");
        };
      };
    }
  };
});
const $lazy_traversableTree = $runtime.binding("traversableTree", () => {
  return {
    Functor0: () => $lazy_functorTree(),
    Foldable1: () => $lazy_foldableTree(),
    traverse: (applicativeMDict) => {
      return ($function) => {
        return (value) => {
          const Apply0Dict = /* @__PURE__ */ applicativeMDict.Apply0();
          const Functor0Dict = /* @__PURE__ */ Apply0Dict.Functor0();
          if (value.tag === "Leaf") {
            const { _1: field0 } = value;
            return /* @__PURE__ */ Data_Functor.map(Functor0Dict)((field0Result) => ({
              tag: "Leaf",
              _1: field0Result
            }))($function(field0));
          }
          if (value.tag === "Branch") {
            const { _1: field0$1, _2: field1 } = value;
            return /* @__PURE__ */ Control_Apply.apply(Apply0Dict)(/* @__PURE__ */ Data_Functor.map(Functor0Dict)((field0Result$1) => (field1Result) => ({
              tag: "Branch",
              _1: field0Result$1,
              _2: field1Result
            }))(/* @__PURE__ */ Data_Traversable.traverse($lazy_traversableTree())(applicativeMDict)((element) => $function(element))(field0$1)))(/* @__PURE__ */ Data_Traversable.traverse($lazy_traversableTree())(applicativeMDict)((element$1) => $function(element$1))(field1));
          }
          throw new Error("Pattern match failure");
        };
      };
    },
    sequence: (applicativeMDict$1) => (value$1) => /* @__PURE__ */ Data_Traversable.traverse($lazy_traversableTree())(applicativeMDict$1)((effect0) => effect0)(value$1)
  };
});
export const functorTree = $lazy_functorTree();
export const foldableTree = $lazy_foldableTree();
export const traversableTree = $lazy_traversableTree();
