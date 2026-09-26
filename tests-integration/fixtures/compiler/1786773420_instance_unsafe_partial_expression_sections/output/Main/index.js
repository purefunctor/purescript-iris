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
  const $closure = (composeArgument) => {
    const $closure$1 = (section54) => {
      const $closure$2 = (partialDict) => {
        const $closure$3 = (partialDict$1) => {
          if (section54.tag === "Sentence") {
            const { _1: content } = section54;
            return content;
          }
          throw new Error("Pattern match failure");
        };
        return /* @__PURE__ */ $closure$3(partialDict);
      };
      return $closure$2;
    };
    return /* @__PURE__ */ Partial_Unsafe.unsafePartial(/* @__PURE__ */ $closure$1(composeArgument));
  };
  const $closure$4 = (partialDict$2) => {
    const $closure$5 = (content$1) => {
      if (content$1.tag === "Sentence") {
        const { _1: value } = content$1;
        return value;
      }
      throw new Error("Pattern match failure");
    };
    return $closure$5;
  };
  const $closure$6 = (content$2) => {
    const $closure$7 = (partialDict$3) => {
      if (content$2.tag === "Sentence") {
        const { _1: value$1 } = content$2;
        return value$1;
      }
      throw new Error("Pattern match failure");
    };
    return Partial_Unsafe.unsafePartial($closure$7);
  };
  return {
    fromContent: $closure,
    fromContentPartialLambda: Partial_Unsafe.unsafePartial($closure$4),
    fromContentLambdaPartial: $closure$6
  };
})();
export const contentOverloadArrayString = /* @__PURE__ */ (() => {
  const $closure = (composeArgument) => {
    const $closure$1 = (section120) => {
      const $closure$2 = (partialDict) => {
        const $closure$3 = (partialDict$1) => {
          if (section120.tag === "Paragraph") {
            const { _1: content } = section120;
            return content;
          }
          throw new Error("Pattern match failure");
        };
        return /* @__PURE__ */ $closure$3(partialDict);
      };
      return $closure$2;
    };
    return /* @__PURE__ */ Partial_Unsafe.unsafePartial(/* @__PURE__ */ $closure$1(composeArgument));
  };
  const $closure$4 = (partialDict$2) => {
    const $closure$5 = (content$1) => {
      if (content$1.tag === "Paragraph") {
        const { _1: value } = content$1;
        return value;
      }
      throw new Error("Pattern match failure");
    };
    return $closure$5;
  };
  const $closure$6 = (content$2) => {
    const $closure$7 = (partialDict$3) => {
      if (content$2.tag === "Paragraph") {
        const { _1: value$1 } = content$2;
        return value$1;
      }
      throw new Error("Pattern match failure");
    };
    return Partial_Unsafe.unsafePartial($closure$7);
  };
  return {
    fromContent: $closure,
    fromContentPartialLambda: Partial_Unsafe.unsafePartial($closure$4),
    fromContentLambdaPartial: $closure$6
  };
})();
