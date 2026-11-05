import * as Data_Bifunctor from "../Data.Bifunctor/index.js";
import * as Data_Functor from "../Data.Functor/index.js";
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
export const Const2 = ($value0) => ({
  tag: "Const2",
  _1: $value0
});
export const WrapBoth = ($value0) => ($value1) => ({
  tag: "WrapBoth",
  _1: $value0,
  _2: $value1
});
export const Tuple = ($value0) => ($value1) => ({
  tag: "Tuple",
  _1: $value0,
  _2: $value1
});
export const Both = ($value0) => ({
  tag: "Both",
  _1: $value0
});
export const OneSidedFirst = ($value0) => ({
  tag: "OneSidedFirst",
  _1: $value0
});
export const OneSidedSecond = ($value0) => ({
  tag: "OneSidedSecond",
  _1: $value0
});
export const Box = ($value0) => ({
  tag: "Box",
  _1: $value0
});
export const Nested = ($value0) => ({
  tag: "Nested",
  _1: $value0
});
export const Triple = ($value0) => ($value1) => ($value2) => ({
  tag: "Triple",
  _1: $value0,
  _2: $value1,
  _3: $value2
});
export const NestedTriple = ($value0) => ({
  tag: "NestedTriple",
  _1: $value0
});
export const NestedTripleLast = ($value0) => ({
  tag: "NestedTripleLast",
  _1: $value0
});
export const ReaderPair = ($value0) => ({
  tag: "ReaderPair",
  _1: $value0
});
export const RecordPair = ($value0) => ({
  tag: "RecordPair",
  _1: $value0
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
export const bifunctorTuple = /* @__PURE__ */ (() => {
  return { bimap: (firstFunction) => {
    return (secondFunction) => {
      return (value) => {
        if (value.tag === "Tuple") {
          const { _1: field0, _2: field1 } = value;
          return {
            tag: "Tuple",
            _1: firstFunction(field0),
            _2: secondFunction(field1)
          };
        }
        throw new Error("Pattern match failure");
      };
    };
  } };
})();
const bifunctorTupleDictBimap = /* @__PURE__ */ Data_Bifunctor.bimap(bifunctorTuple);
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
export const bifunctorConst2TypeType = /* @__PURE__ */ (() => {
  return { bimap: (firstFunction) => {
    return (secondFunction) => {
      return (value) => {
        if (value.tag === "Const2") {
          const { _1: field0 } = value;
          return {
            tag: "Const2",
            _1: field0
          };
        }
        throw new Error("Pattern match failure");
      };
    };
  } };
})();
export const functorTuple = /* @__PURE__ */ (() => {
  return { map: ($function) => {
    return (value) => {
      if (value.tag === "Tuple") {
        const { _1: field0, _2: field1 } = value;
        return {
          tag: "Tuple",
          _1: field0,
          _2: $function(field1)
        };
      }
      throw new Error("Pattern match failure");
    };
  } };
})();
export const bifunctorBoth = /* @__PURE__ */ (() => {
  return { bimap: (firstFunction) => {
    return (secondFunction) => {
      return (value) => {
        if (value.tag === "Both") {
          const { _1: field0 } = value;
          return {
            tag: "Both",
            _1: /* @__PURE__ */ bifunctorTupleDictBimap((element) => firstFunction(element))((element$1) => secondFunction(element$1))(field0)
          };
        }
        throw new Error("Pattern match failure");
      };
    };
  } };
})();
export const bifunctorOneSided = /* @__PURE__ */ (() => {
  return { bimap: (firstFunction) => {
    return (secondFunction) => {
      return (value) => {
        if (value.tag === "OneSidedFirst") {
          const { _1: field0 } = value;
          return {
            tag: "OneSidedFirst",
            _1: /* @__PURE__ */ bifunctorTupleDictBimap((element) => firstFunction(element))((unchanged) => unchanged)(field0)
          };
        }
        if (value.tag === "OneSidedSecond") {
          const { _1: field0$1 } = value;
          return {
            tag: "OneSidedSecond",
            _1: /* @__PURE__ */ Data_Functor.map(functorTuple)((element$1) => secondFunction(element$1))(field0$1)
          };
        }
        throw new Error("Pattern match failure");
      };
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
export const bifunctorNested = /* @__PURE__ */ (() => {
  return { bimap: (firstFunction) => {
    return (secondFunction) => {
      return (value) => {
        if (value.tag === "Nested") {
          const { _1: field0 } = value;
          return {
            tag: "Nested",
            _1: /* @__PURE__ */ Data_Functor.map(functorBox)((element) => /* @__PURE__ */ bifunctorTupleDictBimap((element$1) => firstFunction(element$1))((element$2) => secondFunction(element$2))(element))(field0)
          };
        }
        throw new Error("Pattern match failure");
      };
    };
  } };
})();
export const bifunctorTriple = /* @__PURE__ */ (() => {
  return { bimap: (firstFunction) => {
    return (secondFunction) => {
      return (value) => {
        if (value.tag === "Triple") {
          const { _1: field0, _2: field1, _3: field2 } = value;
          return {
            tag: "Triple",
            _1: field0,
            _2: firstFunction(field1),
            _3: secondFunction(field2)
          };
        }
        throw new Error("Pattern match failure");
      };
    };
  } };
})();
export const functorTriple = /* @__PURE__ */ (() => {
  return { map: ($function) => {
    return (value) => {
      if (value.tag === "Triple") {
        const { _1: field0, _2: field1, _3: field2 } = value;
        return {
          tag: "Triple",
          _1: field0,
          _2: field1,
          _3: $function(field2)
        };
      }
      throw new Error("Pattern match failure");
    };
  } };
})();
export const bifunctorNestedTriple = /* @__PURE__ */ (() => {
  return { bimap: (firstFunction) => {
    return (secondFunction) => {
      return (value) => {
        if (value.tag === "NestedTriple") {
          const { _1: field0 } = value;
          return {
            tag: "NestedTriple",
            _1: /* @__PURE__ */ Data_Bifunctor.bimap(bifunctorTriple)((element) => firstFunction(element))((element$1) => secondFunction(element$1))(field0)
          };
        }
        throw new Error("Pattern match failure");
      };
    };
  } };
})();
export const bifunctorNestedTripleLastType = /* @__PURE__ */ (() => {
  return { bimap: (firstFunction) => {
    return (secondFunction) => {
      return (value) => {
        if (value.tag === "NestedTripleLast") {
          const { _1: field0 } = value;
          return {
            tag: "NestedTripleLast",
            _1: /* @__PURE__ */ Data_Functor.map(functorTriple)((element) => secondFunction(element))(field0)
          };
        }
        throw new Error("Pattern match failure");
      };
    };
  } };
})();
export const bifunctorReaderPair = /* @__PURE__ */ (() => {
  return { bimap: (firstFunction) => {
    return (secondFunction) => {
      return (value) => {
        if (value.tag === "ReaderPair") {
          const { _1: field0 } = value;
          return {
            tag: "ReaderPair",
            _1: (argument) => /* @__PURE__ */ bifunctorTupleDictBimap((element) => firstFunction(element))((element$1) => secondFunction(element$1))(field0(argument))
          };
        }
        throw new Error("Pattern match failure");
      };
    };
  } };
})();
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
