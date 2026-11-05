export const Just = ($value0) => ({
  tag: "Just",
  _1: $value0
});
export const Nothing = "Nothing";
export const NonEmpty = ($value0) => ($value1) => ({
  tag: "NonEmpty",
  _1: $value0,
  _2: $value1
});
export function foldMap(dictionary) {
  return dictionary.foldMap;
}
export function foldMapWithIndex(dictionary) {
  return dictionary.foldMapWithIndex;
}
export function foldlWithIndex(dictionary) {
  return dictionary.foldlWithIndex;
}
export function foldrWithIndex(dictionary) {
  return dictionary.foldrWithIndex;
}
export function foldableNonEmpty(foldableFDict) {
  return { foldMap: (f) => {
    return ($nonEmpty) => {
      if ($nonEmpty.tag === "NonEmpty") {
        const { _1: a, _2: fa } = $nonEmpty;
        return f(a);
      } else {
        throw new Error("Pattern match failure");
      }
    };
  } };
}
export function foldableWithIndexMaybeNonEmpty(foldableWithIndexIFDict) {
  return {
    Foldable0: () => /* @__PURE__ */ foldableNonEmpty(/* @__PURE__ */ foldableWithIndexIFDict.Foldable0()),
    foldMapWithIndex: (f) => {
      return ($nonEmpty) => {
        if ($nonEmpty.tag === "NonEmpty") {
          const { _1: a, _2: fa } = $nonEmpty;
          return f("Nothing")(a);
        } else {
          throw new Error("Pattern match failure");
        }
      };
    },
    foldlWithIndex: (f$1) => {
      return (b) => {
        return ($nonEmpty$1) => {
          if ($nonEmpty$1.tag === "NonEmpty") {
            const { _1: a$1, _2: fa$1 } = $nonEmpty$1;
            return /* @__PURE__ */ foldlWithIndex(foldableWithIndexIFDict)((composeArgument) => /* @__PURE__ */ f$1({
              tag: "Just",
              _1: composeArgument
            }))(f$1("Nothing")(b)(a$1))(fa$1);
          } else {
            throw new Error("Pattern match failure");
          }
        };
      };
    },
    foldrWithIndex: (f$2) => {
      return (b$1) => {
        return ($nonEmpty$2) => {
          if ($nonEmpty$2.tag === "NonEmpty") {
            const { _1: a$2, _2: fa$2 } = $nonEmpty$2;
            return f$2("Nothing")(a$2)(/* @__PURE__ */ foldrWithIndex(foldableWithIndexIFDict)((composeArgument$1) => /* @__PURE__ */ f$2({
              tag: "Just",
              _1: composeArgument$1
            }))(b$1)(fa$2));
          } else {
            throw new Error("Pattern match failure");
          }
        };
      };
    }
  };
}
