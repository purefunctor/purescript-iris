export function pick(dictionary) {
  return dictionary.pick;
}
export function pickInt(showDict) {
  return { pick: ($int) => "int" };
}
export const pick1 = { pick: ($a) => "other" };
export const picksInt = (() => {
  let $result;
  throw new Error("Generated code reached a source error");
  return /* @__PURE__ */ pick($result)(1 | 0);
})();
export const picksString = (() => {
  let $result;
  throw new Error("Generated code reached a source error");
  return /* @__PURE__ */ pick($result)("string");
})();
