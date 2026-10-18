export function eq(dictionary) {
  return dictionary.eq;
}
export function compare(dictionary) {
  return dictionary.compare;
}
export function test1(eqDict) {
  return (x) => /* @__PURE__ */ eq(eqDict)(x)(x) ? /* @__PURE__ */ eq(eqDict)(x)(x) : false;
}
export function test2(ordDict) {
  return (eqDict) => {
    return (x) => /* @__PURE__ */ eq(eqDict)(x)(x) ? /* @__PURE__ */ compare(ordDict)(x)(x) : /* @__PURE__ */ compare(ordDict)(x)(x);
  };
}
