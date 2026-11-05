import * as Partial_Unsafe from "../Partial.Unsafe/index.js";
export const Sentence = ($value0) => ({
  tag: "Sentence",
  _1: $value0
});
export const Paragraph = ($value0) => ({
  tag: "Paragraph",
  _1: $value0
});
export function fromContent(dictionary) {
  return dictionary.fromContent;
}
export function fromContentPartialLambda(dictionary) {
  return dictionary.fromContentPartialLambda;
}
export function fromContentLambdaPartial(dictionary) {
  return dictionary.fromContentLambdaPartial;
}
export const contentOverloadString = /* @__PURE__ */ (() => {
  return {
    fromContent: (composeArgument) => {
      return /* @__PURE__ */ Partial_Unsafe.unsafePartial(/* @__PURE__ */ ((section54) => {
        return (partialDict) => {
          return /* @__PURE__ */ ((partialDict$1) => {
            if (section54.tag === "Sentence") {
              const { _1: content } = section54;
              return content;
            }
            throw new Error("Pattern match failure");
          })(partialDict);
        };
      })(composeArgument));
    },
    fromContentPartialLambda: (content$1) => {
      if (content$1.tag === "Sentence") {
        const { _1: value } = content$1;
        return value;
      }
      throw new Error("Pattern match failure");
    },
    fromContentLambdaPartial: (content$2) => {
      if (content$2.tag === "Sentence") {
        const { _1: value$1 } = content$2;
        return value$1;
      }
      throw new Error("Pattern match failure");
    }
  };
})();
export const contentOverloadArrayString = /* @__PURE__ */ (() => {
  return {
    fromContent: (composeArgument) => {
      return /* @__PURE__ */ Partial_Unsafe.unsafePartial(/* @__PURE__ */ ((section120) => {
        return (partialDict) => {
          return /* @__PURE__ */ ((partialDict$1) => {
            if (section120.tag === "Paragraph") {
              const { _1: content } = section120;
              return content;
            }
            throw new Error("Pattern match failure");
          })(partialDict);
        };
      })(composeArgument));
    },
    fromContentPartialLambda: (content$1) => {
      if (content$1.tag === "Paragraph") {
        const { _1: value } = content$1;
        return value;
      }
      throw new Error("Pattern match failure");
    },
    fromContentLambdaPartial: (content$2) => {
      if (content$2.tag === "Paragraph") {
        const { _1: value$1 } = content$2;
        return value$1;
      }
      throw new Error("Pattern match failure");
    }
  };
})();
