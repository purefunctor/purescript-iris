export function eq(dictionary) {
  return dictionary.eq;
}
export function compare(dictionary) {
  return dictionary.compare;
}
export function test(ordDict) {
  return (x) => /* @__PURE__ */ eq(/* @__PURE__ */ ordDict.Eq0())(x)(x) ? /* @__PURE__ */ compare(ordDict)(x)(x) : /* @__PURE__ */ compare(ordDict)(x)(x);
}
