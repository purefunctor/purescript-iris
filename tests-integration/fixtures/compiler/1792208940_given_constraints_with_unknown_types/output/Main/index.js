import * as Data_Show from "../Data.Show/index.js";
export function givenRigid(showDict) {
  const $closure = (value) => {
    const $function = Data_Show.show;
    let $result;
    throw new Error("Generated code reached a source error");
    return /* @__PURE__ */ $function($result)(value);
  };
  return $closure;
}
export function givenConcrete(showDict) {
  return /* @__PURE__ */ Data_Show.show(Data_Show.showInt)(1 | 0);
}
export function givenPartial(showArrayDict) {
  const $closure = (value) => {
    const $function = Data_Show.show;
    const $function$1 = Data_Show.showArray;
    let $result;
    throw new Error("Generated code reached a source error");
    return /* @__PURE__ */ $function(/* @__PURE__ */ $function$1($result))(value);
  };
  return $closure;
}
