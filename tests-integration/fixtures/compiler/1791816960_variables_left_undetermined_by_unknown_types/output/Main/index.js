import * as Data_Show from "../Data.Show/index.js";
export function convert(dictionary) {
  return dictionary.convert;
}
export function showsConvert(value) {
  const $function = Data_Show.show;
  let $result;
  throw new Error("Generated code reached a source error");
  const $function$1 = $function($result);
  let $result$1;
  throw new Error("Generated code reached a source error");
  return /* @__PURE__ */ $function$1(/* @__PURE__ */ convert($result$1)(value));
}
export function joint(dictionary) {
  return dictionary.joint;
}
export function showsJoint(value) {
  const $function = Data_Show.show;
  let $result;
  throw new Error("Generated code reached a source error");
  const $function$1 = $function($result);
  let $result$1;
  throw new Error("Generated code reached a source error");
  return /* @__PURE__ */ $function$1(/* @__PURE__ */ joint($result$1)(value)(0 | 0));
}
export function independent(dictionary) {
  return dictionary.independent;
}
export function showsIndependent(value) {
  const $function = Data_Show.show;
  let $result;
  throw new Error("Generated code reached a source error");
  const $function$1 = $function($result);
  let $result$1;
  throw new Error("Generated code reached a source error");
  return /* @__PURE__ */ $function$1(/* @__PURE__ */ independent($result$1)(value)(0 | 0));
}
export const convertIntInt = { convert: (value) => value };
export const jointIntIntString = { joint: ($int) => ($int$1) => "joint" };
export const independentIntIntString = { independent: ($int) => ($int$1) => "independent" };
