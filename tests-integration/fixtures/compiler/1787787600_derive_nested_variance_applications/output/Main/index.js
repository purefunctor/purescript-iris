import * as Data_Bifunctor from "../Data.Bifunctor/index.js";
import * as Data_Functor from "../Data.Functor/index.js";
import * as Data_Functor_Contravariant from "../Data.Functor.Contravariant/index.js";
import * as Data_Profunctor from "../Data.Profunctor/index.js";
export const Predicate = ($value0) => ({
  tag: "Predicate",
  _1: $value0
});
export const UnaryContravariant = ($value0) => ({
  tag: "UnaryContravariant",
  _1: $value0
});
export const Box = ($value0) => ({
  tag: "Box",
  _1: $value0
});
export const UnaryCovariant = ($value0) => ({
  tag: "UnaryCovariant",
  _1: $value0
});
export const Pair = ($value0) => ($value1) => ({
  tag: "Pair",
  _1: $value0,
  _2: $value1
});
export const BinaryFirst = ($value0) => ({
  tag: "BinaryFirst",
  _1: $value0
});
export const BinarySecond = ($value0) => ({
  tag: "BinarySecond",
  _1: $value0
});
export const BinaryBoth = ($value0) => ({
  tag: "BinaryBoth",
  _1: $value0
});
export const FunctionLike = ($value0) => ({
  tag: "FunctionLike",
  _1: $value0
});
export const NestedProfunctor = ($value0) => ({
  tag: "NestedProfunctor",
  _1: $value0
});
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
const bifunctorPairDictBimap = /* @__PURE__ */ Data_Bifunctor.bimap(bifunctorPair);
export const contravariantPredicate = /* @__PURE__ */ (() => {
  return { cmap: ($function) => {
    return (value) => {
      if (value.tag === "Predicate") {
        const { _1: field0 } = value;
        return {
          tag: "Predicate",
          _1: (argument) => field0($function(argument))
        };
      }
      throw new Error("Pattern match failure");
    };
  } };
})();
export const contravariantUnaryContravariant = /* @__PURE__ */ (() => {
  return { cmap: ($function) => {
    return (value) => {
      if (value.tag === "UnaryContravariant") {
        const { _1: field0 } = value;
        return {
          tag: "UnaryContravariant",
          _1: /* @__PURE__ */ Data_Functor_Contravariant.cmap(contravariantPredicate)((element) => $function(element))(field0)
        };
      }
      throw new Error("Pattern match failure");
    };
  } };
})();
export const functorBox = /* @__PURE__ */ (() => {
  return { map: ($function) => {
    return (value) => {
      if (value.tag === "Box") {
        const { _1: field0 } = value;
        return {
          tag: "Box",
          _1: $function(field0)
        };
      }
      throw new Error("Pattern match failure");
    };
  } };
})();
export const contravariantUnaryCovariant = /* @__PURE__ */ (() => {
  return { cmap: ($function) => {
    return (value) => {
      if (value.tag === "UnaryCovariant") {
        const { _1: field0 } = value;
        return {
          tag: "UnaryCovariant",
          _1: /* @__PURE__ */ Data_Functor.map(functorBox)((element) => (argument) => element($function(argument)))(field0)
        };
      }
      throw new Error("Pattern match failure");
    };
  } };
})();
export const functorPair = /* @__PURE__ */ (() => {
  return { map: ($function) => {
    return (value) => {
      if (value.tag === "Pair") {
        const { _1: field0, _2: field1 } = value;
        return {
          tag: "Pair",
          _1: field0,
          _2: $function(field1)
        };
      }
      throw new Error("Pattern match failure");
    };
  } };
})();
export const contravariantBinaryFirst = /* @__PURE__ */ (() => {
  return { cmap: ($function) => {
    return (value) => {
      if (value.tag === "BinaryFirst") {
        const { _1: field0 } = value;
        return {
          tag: "BinaryFirst",
          _1: /* @__PURE__ */ bifunctorPairDictBimap((element) => (argument) => element($function(argument)))((unchanged) => unchanged)(field0)
        };
      }
      throw new Error("Pattern match failure");
    };
  } };
})();
export const contravariantBinarySecond = /* @__PURE__ */ (() => {
  return { cmap: ($function) => {
    return (value) => {
      if (value.tag === "BinarySecond") {
        const { _1: field0 } = value;
        return {
          tag: "BinarySecond",
          _1: /* @__PURE__ */ Data_Functor.map(functorPair)((element) => (argument) => element($function(argument)))(field0)
        };
      }
      throw new Error("Pattern match failure");
    };
  } };
})();
export const contravariantBinaryBoth = /* @__PURE__ */ (() => {
  return { cmap: ($function) => {
    return (value) => {
      if (value.tag === "BinaryBoth") {
        const { _1: field0 } = value;
        return {
          tag: "BinaryBoth",
          _1: /* @__PURE__ */ bifunctorPairDictBimap((element) => (argument) => element($function(argument)))((element$1) => (argument$1) => element$1($function(argument$1)))(field0)
        };
      }
      throw new Error("Pattern match failure");
    };
  } };
})();
export const profunctorFunctionLike = /* @__PURE__ */ (() => {
  return { dimap: (firstFunction) => {
    return (secondFunction) => {
      return (value) => {
        if (value.tag === "FunctionLike") {
          const { _1: field0 } = value;
          return {
            tag: "FunctionLike",
            _1: (argument) => secondFunction(field0(firstFunction(argument)))
          };
        }
        throw new Error("Pattern match failure");
      };
    };
  } };
})();
export const profunctorNestedProfunctor = /* @__PURE__ */ (() => {
  return { dimap: (firstFunction) => {
    return (secondFunction) => {
      return (value) => {
        if (value.tag === "NestedProfunctor") {
          const { _1: field0 } = value;
          return {
            tag: "NestedProfunctor",
            _1: /* @__PURE__ */ Data_Profunctor.dimap(profunctorFunctionLike)((element) => firstFunction(element))((element$1) => secondFunction(element$1))(field0)
          };
        }
        throw new Error("Pattern match failure");
      };
    };
  } };
})();
