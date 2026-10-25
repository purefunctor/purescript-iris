import * as Control_Category from "../Control.Category/index.js";
import * as Data_Eq from "../Data.Eq/index.js";
import * as Data_Lens_Iso from "../Data.Lens.Iso/index.js";
import * as Data_Profunctor from "../Data.Profunctor/index.js";
import * as Data_Show from "../Data.Show/index.js";
export function after(prefix) {
  return (showADict) => (value) => prefix + /* @__PURE__ */ Data_Show.show(showADict)(value);
}
export function before(showADict) {
  return (prefix) => (value) => prefix + /* @__PURE__ */ Data_Show.show(showADict)(value);
}
export function branches($boolean) {
  const $closure = (showADict) => {
    const $closure$1 = ($a) => {
      if ($boolean === true) {
        const value = $a;
        return "true=" + /* @__PURE__ */ Data_Show.show(showADict)(value);
      }
      if ($boolean === false) {
        return /* @__PURE__ */ after("false=")(showADict)($a);
      }
      throw new Error("Pattern match failure");
    };
    return $closure$1;
  };
  return $closure;
}
export function interleaved(showADict) {
  return (value) => (eqBDict) => (left) => (right) => /* @__PURE__ */ Data_Eq.eq(eqBDict)(left)(right) ? /* @__PURE__ */ Data_Show.show(showADict)(value) : "different";
}
const categoryFunctionDictIdentity = /* @__PURE__ */ Control_Category.identity(Control_Category.categoryFn);
export const test = /* @__PURE__ */ Data_Lens_Iso.iso(categoryFunctionDictIdentity)(categoryFunctionDictIdentity)(Data_Profunctor.profunctorFn)(categoryFunctionDictIdentity)(42 | 0);
export const partial = /* @__PURE__ */ Data_Lens_Iso.iso((section46) => section46 + (3 | 0) | 0)((section53) => section53 * (2 | 0) | 0)(Data_Profunctor.profunctorFn)((section60) => section60 + (5 | 0) | 0);
export const direct = /* @__PURE__ */ after("value=")(Data_Show.showInt)(42 | 0);
export const applied = /* @__PURE__ */ after("partial=")(Data_Show.showInt);
export const dictionaryFirst = /* @__PURE__ */ before(Data_Show.showInt)("before=")(17 | 0);
export const eta = after;
export const local = /* @__PURE__ */ ((prefix) => (showADict) => (value) => prefix + /* @__PURE__ */ Data_Show.show(showADict)(value))("local=")(Data_Show.showInt)(13 | 0);
