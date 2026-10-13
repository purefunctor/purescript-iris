export function needsLess(compareMPLTDict) {
  return ($proxy) => ($proxy$1) => 0 | 0;
}
export function usesInvalidGivens(compareMInvalidLTDict) {
  return (compareInvalidPLTDict) => {
    return (m) => (p) => /* @__PURE__ */ needsLess({})(m)(p);
  };
}
export function usesNoGivens(m) {
  return (p) => {
    let $result;
    throw new Error("Generated code reached a source error");
    return /* @__PURE__ */ needsLess($result)(m)(p);
  };
}
