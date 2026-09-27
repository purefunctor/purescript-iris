export const Proxy = "Proxy";
export const Pair = ($value0) => ($value1) => ({
  tag: "Pair",
  _1: $value0,
  _2: $value1
});
export const Legacy = "Legacy";
export const Key = "Key";
export function warned(warnTextDict) {
  return (x) => x;
}
export function describe(dictionary) {
  return dictionary.describe;
}
export function describeLegacy(warnTextDict) {
  return { describe: ($legacy) => 0 | 0 };
}
export function lookupProxyType(resolveKVDict) {
  return {};
}
export function lookup(lookupProxyKeyBDict) {
  return "Proxy";
}
export function warnResolved(warnBesideTextQuoteBDict) {
  return (p) => p;
}
export function consume($proxy) {
  return 0 | 0;
}
export const test = {
  tag: "Pair",
  _1: /* @__PURE__ */ warned({})(1 | 0),
  _2: /* @__PURE__ */ warned({})(2 | 0)
};
export const test2 = /* @__PURE__ */ describe(/* @__PURE__ */ describeLegacy({}))("Legacy");
export const resolveKeyInt = {};
export const test3 = consume(/* @__PURE__ */ warnResolved({})(/* @__PURE__ */ lookup(/* @__PURE__ */ lookupProxyType(resolveKeyInt))));
