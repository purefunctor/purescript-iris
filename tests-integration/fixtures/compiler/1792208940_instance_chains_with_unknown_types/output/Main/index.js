export function pick(dictionary) {
  return dictionary.pick;
}
export function single(dictionary) {
  return dictionary.single;
}
export const pick1 = { pick: ($a) => "other" };
const pickStringDictPick = /* @__PURE__ */ pick(pick1);
export const pickBoolean = { pick: ($boolean) => "boolean" };
export const pickArray = { pick: ($array) => "invalid" };
export const picksBoolean = /* @__PURE__ */ pick(pickBoolean)(true);
export const picksString = /* @__PURE__ */ pickStringDictPick("string");
export const picksArray = /* @__PURE__ */ pickStringDictPick([1 | 0]);
export const single1 = { single: ($argument0) => "invalid" };
export const singleBoolean1 = { single: ($boolean) => "boolean" };
export const singleInt = (() => {
  let $result;
  throw new Error("Generated code reached a source error");
  return /* @__PURE__ */ single($result)(1 | 0);
})();
export const singleBoolean = /* @__PURE__ */ single(singleBoolean1)(true);
