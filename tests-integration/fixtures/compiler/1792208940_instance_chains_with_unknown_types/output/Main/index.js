export function pick(dictionary) {
  return dictionary.pick;
}
export function single(dictionary) {
  return dictionary.single;
}
export const pickBoolean = { pick: ($boolean) => "boolean" };
export const pickArray = { pick: ($array) => "invalid" };
export const pick1 = { pick: ($a) => "other" };
export const picksBoolean = (() => {
  let $result;
  throw new Error("Generated code reached a source error");
  return /* @__PURE__ */ pick($result)(true);
})();
export const picksString = (() => {
  let $result;
  throw new Error("Generated code reached a source error");
  return /* @__PURE__ */ pick($result)("string");
})();
export const picksArray = (() => {
  let $result;
  throw new Error("Generated code reached a source error");
  return /* @__PURE__ */ pick($result)([1 | 0]);
})();
export const single1 = { single: ($argument0) => "invalid" };
export const singleBoolean1 = { single: ($boolean) => "boolean" };
export const singleInt = (() => {
  let $result;
  throw new Error("Generated code reached a source error");
  return /* @__PURE__ */ single($result)(1 | 0);
})();
export const singleBoolean = /* @__PURE__ */ single(singleBoolean1)(true);
