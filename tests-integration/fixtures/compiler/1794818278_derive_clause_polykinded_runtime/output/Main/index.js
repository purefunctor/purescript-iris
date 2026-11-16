import * as Control_Applicative from "../Control.Applicative/index.js";
import * as Data_Eq from "../Data.Eq/index.js";
import * as Data_Foldable from "../Data.Foldable/index.js";
import * as Data_Functor from "../Data.Functor/index.js";
import * as Data_Functor_Contravariant from "../Data.Functor.Contravariant/index.js";
import * as Data_Maybe from "../Data.Maybe/index.js";
import * as Data_Monoid from "../Data.Monoid/index.js";
import * as Data_Ord from "../Data.Ord/index.js";
import * as Data_Ordering from "../Data.Ordering/index.js";
import * as Data_Ring from "../Data.Ring/index.js";
import * as Data_Traversable from "../Data.Traversable/index.js";
import * as $runtime from "../runtime.js";
export const Phantom = ($value0) => ({
  tag: "Phantom",
  _1: $value0
});
const $lazy_traversablePhantomType = $runtime.binding("traversablePhantomType", () => {
  return {
    Functor0: () => functorPhantomType,
    Foldable1: () => foldablePhantomType,
    traverse: (applicativeMDict) => {
      return ($function) => {
        return (value) => {
          if (value.tag === "Phantom") {
            const { _1: field0 } = value;
            return /* @__PURE__ */ Control_Applicative.pure(applicativeMDict)({
              tag: "Phantom",
              _1: field0
            });
          }
          throw new Error("Pattern match failure");
        };
      };
    },
    sequence: (applicativeMDict$1) => (value$1) => /* @__PURE__ */ Data_Traversable.traverse($lazy_traversablePhantomType())(applicativeMDict$1)((effect0) => effect0)(value$1)
  };
});
export const eqPhantom = /* @__PURE__ */ (() => {
  return { eq: (left) => {
    return (right) => {
      if (left.tag === "Phantom" && right.tag === "Phantom") {
        const { _1: left0 } = left;
        const { _1: right0 } = right;
        if (left0 === right0) {
          return true;
        } else {
          return false;
        }
      }
      throw new Error("Pattern match failure");
    };
  } };
})();
export const eq1PhantomType = { eq1: (eqADict) => /* @__PURE__ */ Data_Eq.eq(eqPhantom) };
export const ordPhantom = /* @__PURE__ */ (() => {
  return {
    Eq0: () => eqPhantom,
    compare: (left) => {
      return (right) => {
        if (left.tag === "Phantom" && right.tag === "Phantom") {
          const { _1: left0 } = left;
          const { _1: right0 } = right;
          const $scrutinee = /* @__PURE__ */ Data_Ord.compare(Data_Ord.ordInt)(left0)(right0);
          if ($scrutinee === "EQ") {
            return "EQ";
          }
          const ordering = $scrutinee;
          return ordering;
        }
        throw new Error("Pattern match failure");
      };
    }
  };
})();
export const ord1PhantomType = {
  Eq10: () => eq1PhantomType,
  compare1: (ordADict) => /* @__PURE__ */ Data_Ord.compare(ordPhantom)
};
export const functorPhantomType = /* @__PURE__ */ (() => {
  return { map: ($function) => {
    return (value) => {
      if (value.tag === "Phantom") {
        const { _1: field0 } = value;
        return {
          tag: "Phantom",
          _1: field0
        };
      }
      throw new Error("Pattern match failure");
    };
  } };
})();
export const foldablePhantomType = /* @__PURE__ */ (() => {
  return {
    foldr: ($function) => {
      return (accumulator) => {
        return (value) => {
          if (value.tag === "Phantom") {
            const { _1: field0 } = value;
            return accumulator;
          }
          throw new Error("Pattern match failure");
        };
      };
    },
    foldl: ($function$1) => {
      return (accumulator$1) => {
        return (value$1) => {
          if (value$1.tag === "Phantom") {
            const { _1: field0$1 } = value$1;
            return accumulator$1;
          }
          throw new Error("Pattern match failure");
        };
      };
    },
    foldMap: (monoidMDict) => {
      return ($function$2) => {
        return (value$2) => {
          if (value$2.tag === "Phantom") {
            const { _1: field0$2 } = value$2;
            return /* @__PURE__ */ Data_Monoid.mempty(monoidMDict);
          }
          throw new Error("Pattern match failure");
        };
      };
    }
  };
})();
export const traversablePhantomType = $lazy_traversablePhantomType();
export const contravariantPhantomType = /* @__PURE__ */ (() => {
  return { cmap: ($function) => {
    return (value) => {
      if (value.tag === "Phantom") {
        const { _1: field0 } = value;
        return {
          tag: "Phantom",
          _1: field0
        };
      }
      throw new Error("Pattern match failure");
    };
  } };
})();
export const mapped = (() => {
  const $scrutinee = /* @__PURE__ */ Data_Functor.map(functorPhantomType)((section79) => section79 + (2 | 0) | 0)({
    tag: "Phantom",
    _1: 13 | 0
  });
  if ($scrutinee.tag === "Phantom") {
    const { _1: n } = $scrutinee;
    return n;
  }
  throw new Error("Pattern match failure");
})();
export const folded = /* @__PURE__ */ Data_Foldable.foldl(foldablePhantomType)(/* @__PURE__ */ Data_Ring.sub(Data_Ring.ringInt))(17 | 0)({
  tag: "Phantom",
  _1: 13 | 0
});
export const traversed = (() => {
  const $scrutinee = /* @__PURE__ */ Data_Traversable.traverse($lazy_traversablePhantomType())(Data_Maybe.applicativeMaybe)((n) => ({
    tag: "Just",
    _1: n + (1 | 0) | 0
  }))({
    tag: "Phantom",
    _1: 13 | 0
  });
  if ($scrutinee.tag === "Just" && $scrutinee._1.tag === "Phantom") {
    const { _1: n$1 } = $scrutinee._1;
    return n$1;
  }
  if ($scrutinee === "Nothing") {
    return 0 | 0;
  }
  throw new Error("Pattern match failure");
})();
export const contramapped = (() => {
  const $scrutinee = /* @__PURE__ */ Data_Functor_Contravariant.cmap(contravariantPhantomType)((section183) => section183 + (2 | 0) | 0)({
    tag: "Phantom",
    _1: 13 | 0
  });
  if ($scrutinee.tag === "Phantom") {
    const { _1: n } = $scrutinee;
    return n;
  }
  throw new Error("Pattern match failure");
})();
export const equal = /* @__PURE__ */ Data_Eq.eq1(eq1PhantomType)(Data_Eq.eqInt)({
  tag: "Phantom",
  _1: 13 | 0
})({
  tag: "Phantom",
  _1: 14 | 0
});
export const ordered = /* @__PURE__ */ Data_Eq.eq(Data_Ordering.eqOrdering)(/* @__PURE__ */ Data_Ord.compare1(ord1PhantomType)(Data_Ord.ordInt)({
  tag: "Phantom",
  _1: 13 | 0
})({
  tag: "Phantom",
  _1: 14 | 0
}))("LT");
