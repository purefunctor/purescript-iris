export function fromGlobal(dictionary) {
  return dictionary.fromGlobal;
}
export function fromLocal(dictionary) {
  return dictionary.fromLocal;
}
export function usesLocal(localInvalidDict) {
  let $result;
  throw new Error("Generated code reached a source error");
  return /* @__PURE__ */ fromLocal($result)(0 | 0);
}
export function usesSkolemLocal(localInvalidDict) {
  const $closure = (value) => {
    let $result;
    throw new Error("Generated code reached a source error");
    return /* @__PURE__ */ fromLocal($result)(value);
  };
  return $closure;
}
export const globalInvalid = { fromGlobal: ($invalid) => 0 | 0 };
export const usesGlobal = (() => {
  let $result;
  throw new Error("Generated code reached a source error");
  return /* @__PURE__ */ fromGlobal($result)(0 | 0);
})();
