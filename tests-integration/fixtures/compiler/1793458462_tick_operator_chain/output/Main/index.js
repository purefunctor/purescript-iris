import * as Control_Category from "../Control.Category/index.js";
function $function(left) {
  return ($int) => {
    return left;
  };
}
export function x(value) {
  const composeInner = /* @__PURE__ */ Control_Category.identity(Control_Category.categoryFn);
  const $result = (composeArgument) => /* @__PURE__ */ $function(/* @__PURE__ */ composeInner(composeArgument));
  return $result(value)(value);
}
export { $function as "function" };
