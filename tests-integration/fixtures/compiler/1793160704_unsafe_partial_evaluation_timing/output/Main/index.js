import * as Partial from "../Partial/index.js";
import * as $foreign from "./foreign.js";
export function tracedValue(value) {
  return traced("after argument")(traced("inside")(value));
}
export function tracedCrash(value) {
  return /* @__PURE__ */ Partial.crashWith({})(traced("message")("crashed")) + traced("unreached")(value) | 0;
}
export const traced = $foreign["traced"];
export const tracedFunction = traced("function")((value) => traced("body")(value));
