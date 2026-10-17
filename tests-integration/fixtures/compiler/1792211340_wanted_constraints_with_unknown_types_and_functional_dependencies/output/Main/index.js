import * as Data_Show from "../Data.Show/index.js";
export function convert(dictionary) {
  return dictionary.convert;
}
export function showsConverted(value) {
  return /* @__PURE__ */ Data_Show.show(Data_Show.showInt)(/* @__PURE__ */ convertIntDictConvert(value));
}
export function inferredConverted(value) {
  return /* @__PURE__ */ convertIntDictConvert(value);
}
export const convertInt = { convert: ($a) => 0 | 0 };
const convertIntDictConvert = /* @__PURE__ */ convert(convertInt);
