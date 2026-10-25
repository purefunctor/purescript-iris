import * as Control_Category from "../Control.Category/index.js";
import * as Data_Unit from "../Data.Unit/index.js";
export function nestedBind(left) {
  return (right) => {
    const total = left + right | 0;
    const $value = Data_Unit.unit;
    const $effect = () => {
      return total;
    };
    const $result = $effect;
    return () => {
      const value = $result();
      return value;
    };
  };
}
export function nestedMap(left) {
  return (right) => {
    const total = left + right | 0;
    const $value = Data_Unit.unit;
    const $effect = () => {
      return total;
    };
    const $result = $effect;
    return () => {
      return categoryFunctionDictIdentity($result());
    };
  };
}
export function nestedApply(left) {
  return (right) => {
    const total = left + right | 0;
    const $value = Data_Unit.unit;
    const $effect = () => {
      return total;
    };
    const $result = $effect;
    return () => {
      let $function;
      $function = categoryFunctionDictIdentity;
      return $function($result());
    };
  };
}
const categoryFunctionDictIdentity = /* @__PURE__ */ Control_Category.identity(Control_Category.categoryFn);
