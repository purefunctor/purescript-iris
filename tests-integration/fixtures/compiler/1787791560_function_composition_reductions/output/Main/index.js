import * as Lookalike from "../Lookalike/index.js";
import * as $foreign from "./foreign.js";
export function composeOrder($boolean) {
  const composeOuter = observe("outer")((value) => value);
  const composeInner = observe("inner")((value$1) => value$1);
  const $result = (composeArgument) => /* @__PURE__ */ composeOuter(/* @__PURE__ */ composeInner(composeArgument));
  return $result(observe("argument")(42 | 0));
}
export function partiallyComposedOrder($boolean) {
  const composeOuter = observe("outer")((value) => value);
  const composeInner = observe("inner")((value$1) => value$1);
  const composedFunction = (composeArgument) => {
    return /* @__PURE__ */ composeOuter(/* @__PURE__ */ composeInner(composeArgument));
  };
  return composedFunction(observe("argument")(42 | 0));
}
export function flippedComposeOrder($boolean) {
  const composeInner = observe("inner")((value) => value);
  const composeOuter = observe("outer")((value$1) => value$1);
  const $result = (composeArgument) => /* @__PURE__ */ composeOuter(/* @__PURE__ */ composeInner(composeArgument));
  return $result(observe("argument")(42 | 0));
}
export function partiallyFlippedComposedOrder($boolean) {
  const composeInner = observe("inner")((value) => value);
  const composeOuter = observe("outer")((value$1) => value$1);
  const composedFunction = (composeArgument) => {
    return /* @__PURE__ */ composeOuter(/* @__PURE__ */ composeInner(composeArgument));
  };
  return composedFunction(observe("argument")(42 | 0));
}
export const observe = $foreign["observe"];
export const readTrace = $foreign["readTrace"];
export const composed = ((composeArgument) => /* @__PURE__ */ ((value) => value)(/* @__PURE__ */ ((value$1) => value$1)(composeArgument)))(42 | 0);
export const flippedComposed = ((composeArgument) => /* @__PURE__ */ ((value) => value)(/* @__PURE__ */ ((value$1) => value$1)(composeArgument)))(42 | 0);
export const lookalikeCompose = Lookalike.compose((value) => value)((value$1) => value$1)(42 | 0);
export const lookalikeComposeFlipped = Lookalike.composeFlipped((value) => value)((value$1) => value$1)(42 | 0);
