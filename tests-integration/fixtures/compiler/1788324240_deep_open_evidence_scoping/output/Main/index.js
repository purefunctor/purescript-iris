export const Proxy = "Proxy";
export function base(dictionary) {
  return dictionary.base;
}
export function chain(dictionary) {
  return dictionary.chain;
}
export function chainZero(baseValueDict) {
  return { chain: /* @__PURE__ */ base(baseValueDict) };
}
export function chainNext(addPreviousCurrentDict) {
  return (chainValuePreviousDict) => {
    return { chain: 0 | 0 };
  };
}
export function chainValue(chainValueNumberDict) {
  return ($proxy) => ($proxy$1) => /* @__PURE__ */ chain(chainValueNumberDict);
}
export function useTwice(baseValueDict) {
  return (proxy) => {
    const chainZeroDict = /* @__PURE__ */ chainZero(baseValueDict);
    const chainNextDict = /* @__PURE__ */ chainNext({})(/* @__PURE__ */ chainNext({})(/* @__PURE__ */ chainNext({})(/* @__PURE__ */ chainNext({})(/* @__PURE__ */ chainNext({})(/* @__PURE__ */ chainNext({})(/* @__PURE__ */ chainNext({})(/* @__PURE__ */ chainNext({})(/* @__PURE__ */ chainNext({})(/* @__PURE__ */ chainNext({})(/* @__PURE__ */ chainNext({})(/* @__PURE__ */ chainNext({})(/* @__PURE__ */ chainNext({})(/* @__PURE__ */ chainNext({})(/* @__PURE__ */ chainNext({})(/* @__PURE__ */ chainNext({})(/* @__PURE__ */ chainNext({})(/* @__PURE__ */ chainNext({})(/* @__PURE__ */ chainNext({})(/* @__PURE__ */ chainNext({})(/* @__PURE__ */ chainNext({})(/* @__PURE__ */ chainNext({})(/* @__PURE__ */ chainNext({})(/* @__PURE__ */ chainNext({})(/* @__PURE__ */ chainNext({})(/* @__PURE__ */ chainNext({})(/* @__PURE__ */ chainNext({})(/* @__PURE__ */ chainNext({})(/* @__PURE__ */ chainNext({})(/* @__PURE__ */ chainNext({})(/* @__PURE__ */ chainNext({})(chainZeroDict)))))))))))))))))))))))))))))));
    const chainNextDict$1 = /* @__PURE__ */ chainNext({})(/* @__PURE__ */ chainNext({})(/* @__PURE__ */ chainNext({})(/* @__PURE__ */ chainNext({})(/* @__PURE__ */ chainNext({})(/* @__PURE__ */ chainNext({})(/* @__PURE__ */ chainNext({})(/* @__PURE__ */ chainNext({})(/* @__PURE__ */ chainNext({})(/* @__PURE__ */ chainNext({})(/* @__PURE__ */ chainNext({})(/* @__PURE__ */ chainNext({})(/* @__PURE__ */ chainNext({})(/* @__PURE__ */ chainNext({})(/* @__PURE__ */ chainNext({})(/* @__PURE__ */ chainNext({})(/* @__PURE__ */ chainNext({})(/* @__PURE__ */ chainNext({})(/* @__PURE__ */ chainNext({})(/* @__PURE__ */ chainNext({})(/* @__PURE__ */ chainNext({})(/* @__PURE__ */ chainNext({})(/* @__PURE__ */ chainNext({})(/* @__PURE__ */ chainNext({})(/* @__PURE__ */ chainNext({})(/* @__PURE__ */ chainNext({})(/* @__PURE__ */ chainNext({})(/* @__PURE__ */ chainNext({})(/* @__PURE__ */ chainNext({})(/* @__PURE__ */ chainNext({})(/* @__PURE__ */ chainNext({})(/* @__PURE__ */ chainNext({})(chainNextDict))))))))))))))))))))))))))))))));
    const $result = /* @__PURE__ */ chainNext({})(/* @__PURE__ */ chainNext({})(/* @__PURE__ */ chainNext({})(/* @__PURE__ */ chainNext({})(/* @__PURE__ */ chainNext({})(/* @__PURE__ */ chainNext({})(/* @__PURE__ */ chainNext({})(chainNextDict$1)))))));
    const $element = /* @__PURE__ */ chainValue($result)(proxy)("Proxy");
    const chainNextDict$2 = /* @__PURE__ */ chainNext({})(/* @__PURE__ */ chainNext({})(/* @__PURE__ */ chainNext({})(/* @__PURE__ */ chainNext({})(/* @__PURE__ */ chainNext({})(/* @__PURE__ */ chainNext({})(/* @__PURE__ */ chainNext({})(/* @__PURE__ */ chainNext({})(/* @__PURE__ */ chainNext({})(/* @__PURE__ */ chainNext({})(/* @__PURE__ */ chainNext({})(/* @__PURE__ */ chainNext({})(/* @__PURE__ */ chainNext({})(/* @__PURE__ */ chainNext({})(/* @__PURE__ */ chainNext({})(/* @__PURE__ */ chainNext({})(/* @__PURE__ */ chainNext({})(/* @__PURE__ */ chainNext({})(/* @__PURE__ */ chainNext({})(/* @__PURE__ */ chainNext({})(/* @__PURE__ */ chainNext({})(/* @__PURE__ */ chainNext({})(/* @__PURE__ */ chainNext({})(/* @__PURE__ */ chainNext({})(/* @__PURE__ */ chainNext({})(/* @__PURE__ */ chainNext({})(/* @__PURE__ */ chainNext({})(/* @__PURE__ */ chainNext({})(/* @__PURE__ */ chainNext({})(/* @__PURE__ */ chainNext({})(/* @__PURE__ */ chainNext({})(chainZeroDict)))))))))))))))))))))))))))))));
    const chainNextDict$3 = /* @__PURE__ */ chainNext({})(/* @__PURE__ */ chainNext({})(/* @__PURE__ */ chainNext({})(/* @__PURE__ */ chainNext({})(/* @__PURE__ */ chainNext({})(/* @__PURE__ */ chainNext({})(/* @__PURE__ */ chainNext({})(/* @__PURE__ */ chainNext({})(/* @__PURE__ */ chainNext({})(/* @__PURE__ */ chainNext({})(/* @__PURE__ */ chainNext({})(/* @__PURE__ */ chainNext({})(/* @__PURE__ */ chainNext({})(/* @__PURE__ */ chainNext({})(/* @__PURE__ */ chainNext({})(/* @__PURE__ */ chainNext({})(/* @__PURE__ */ chainNext({})(/* @__PURE__ */ chainNext({})(/* @__PURE__ */ chainNext({})(/* @__PURE__ */ chainNext({})(/* @__PURE__ */ chainNext({})(/* @__PURE__ */ chainNext({})(/* @__PURE__ */ chainNext({})(/* @__PURE__ */ chainNext({})(/* @__PURE__ */ chainNext({})(/* @__PURE__ */ chainNext({})(/* @__PURE__ */ chainNext({})(/* @__PURE__ */ chainNext({})(/* @__PURE__ */ chainNext({})(/* @__PURE__ */ chainNext({})(/* @__PURE__ */ chainNext({})(/* @__PURE__ */ chainNext({})(chainNextDict$2))))))))))))))))))))))))))))))));
    const $result$1 = /* @__PURE__ */ chainNext({})(/* @__PURE__ */ chainNext({})(/* @__PURE__ */ chainNext({})(/* @__PURE__ */ chainNext({})(/* @__PURE__ */ chainNext({})(/* @__PURE__ */ chainNext({})(/* @__PURE__ */ chainNext({})(chainNextDict$3)))))));
    return [$element, /* @__PURE__ */ chainValue($result$1)(proxy)("Proxy")];
  };
}
export const baseInt = { base: 42 | 0 };
export const result = /* @__PURE__ */ useTwice(baseInt)("Proxy");
