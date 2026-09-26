export function compose(dictionary) {
  return dictionary.compose;
}
export function identity(dictionary) {
  return dictionary.identity;
}
export function test(x) {
  return /* @__PURE__ */ categoryFunctionDictIdentity(categoryFunctionDictIdentity)(x);
}
export const semigroupoidFn = { compose: (f) => (g) => (x) => f(g(x)) };
export const categoryFn = {
  Semigroupoid0: () => semigroupoidFn,
  identity: (x) => x
};
const categoryFunctionDictIdentity = /* @__PURE__ */ identity(categoryFn);
