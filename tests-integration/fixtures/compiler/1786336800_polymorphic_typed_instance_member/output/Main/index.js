import * as $foreign from "./foreign.js";
export function convert(dictionary) {
  return dictionary.convert;
}
export function convertExplicit(dictionary) {
  return dictionary.convertExplicit;
}
export function clash(dictionary) {
  return dictionary.clash;
}
export const unsafeCoerce = $foreign["unsafeCoerce"];
export const convertFG = {
  convert: unsafeCoerce,
  convertExplicit: unsafeCoerce
};
export const clash1 = /* @__PURE__ */ (() => {
  return { clash: (value) => {
    const matched = value;
    return unsafeCoerce(matched);
  } };
})();
