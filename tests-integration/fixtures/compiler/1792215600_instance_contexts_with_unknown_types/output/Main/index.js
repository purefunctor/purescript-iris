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
  return /* @__PURE__ */ pick(/* @__PURE__ */ pickInt($result))(1 | 0);
})();
export const picksString = /* @__PURE__ */ pick(pick1)("string");
