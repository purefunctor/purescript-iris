import * as Data_Show from "../Data.Show/index.js";
export function convert(dictionary) {
  return dictionary.convert;
}
export function wantedShow(value) {
  const $function = Data_Show.show;
  let $result;
  throw new Error("Generated code reached a source error");
  return /* @__PURE__ */ $function($result)(value);
}
export function wantedShowArray(value) {
  const $function = Data_Show.show;
  const $function$1 = Data_Show.showArray;
  let $result;
  throw new Error("Generated code reached a source error");
  return /* @__PURE__ */ $function(/* @__PURE__ */ $function$1($result))(value);
}
export function wantedDependency(value) {
  let $result;
  throw new Error("Generated code reached a source error");
  return /* @__PURE__ */ convert($result)(value);
}
export const convertIntString = { convert: ($int) => "int" };
