import * as Control_Category from "../Control.Category/index.js";
import * as Data_Unit from "../Data.Unit/index.js";
export function nestedBind(left) {
  return (right) => {
    const $value = Data_Unit.unit;
    return () => {
      let value;
      const $value$1 = left + right | 0;
      value = $value$1;
      return value;
    };
  };
}
export function nestedMap(left) {
  return (right) => {
    const $value = Data_Unit.unit;
    return () => {
      let $value$1;
      const $value$2 = left + right | 0;
      $value$1 = $value$2;
      return categoryFunctionDictIdentity($value$1);
    };
  };
}
export function nestedApply(left) {
  return (right) => {
    const $value = Data_Unit.unit;
    return () => {
      const $function = categoryFunctionDictIdentity;
      let $argument;
      const $value$1 = left + right | 0;
      $argument = $value$1;
      return $function($argument);
    };
  };
}
const categoryFunctionDictIdentity = /* @__PURE__ */ Control_Category.identity(Control_Category.categoryFn);
