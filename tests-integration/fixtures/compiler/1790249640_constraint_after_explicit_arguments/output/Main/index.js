import * as Control_Category from "../Control.Category/index.js";
import * as Data_Eq from "../Data.Eq/index.js";
import * as Data_Lens_Iso from "../Data.Lens.Iso/index.js";
import * as Data_Profunctor from "../Data.Profunctor/index.js";
import * as Data_Semigroup from "../Data.Semigroup/index.js";
import * as Data_Show from "../Data.Show/index.js";
export function after(showADict) {
  return (prefix) => (value) => /* @__PURE__ */ semigroupStringDictAppend(prefix)(/* @__PURE__ */ Data_Show.show(showADict)(value));
}
export function before(showADict) {
  return (prefix) => (value) => /* @__PURE__ */ semigroupStringDictAppend(prefix)(/* @__PURE__ */ Data_Show.show(showADict)(value));
}
export function eta(showADict) {
  return after;
}
export function branches(showADict) {
  const $closure = ($boolean) => {
    return ($a) => {
      if ($boolean === true) {
        const value = $a;
        return /* @__PURE__ */ semigroupStringDictAppend("true=")(/* @__PURE__ */ Data_Show.show(showADict)(value));
      }
      if ($boolean === false) {
        return /* @__PURE__ */ after("false=")(showADict)($a);
      }
      throw new Error("Pattern match failure");
    };
  };
  return $closure;
}
export function interleaved(showADict) {
  return (eqBDict) => {
    const $closure = (value) => {
      return (left) => {
        return (right) => {
          if (/* @__PURE__ */ Data_Eq.eq(eqBDict)(left)(right)) {
            return /* @__PURE__ */ Data_Show.show(showADict)(value);
          } else {
            return "different";
          }
        };
      };
    };
    return $closure;
  };
}
const categoryFunctionDictIdentity = /* @__PURE__ */ Control_Category.identity(Control_Category.categoryFn);
const semigroupStringDictAppend = /* @__PURE__ */ Data_Semigroup.append(Data_Semigroup.semigroupString);
export const test = /* @__PURE__ */ Data_Lens_Iso.iso(categoryFunctionDictIdentity)(categoryFunctionDictIdentity)(Data_Profunctor.profunctorFn)(categoryFunctionDictIdentity)(42 | 0);
export const partial = /* @__PURE__ */ Data_Lens_Iso.iso((section46) => section46 + (3 | 0) | 0)((section53) => section53 * (2 | 0) | 0)(Data_Profunctor.profunctorFn)((section60) => section60 + (5 | 0) | 0);
export const direct = /* @__PURE__ */ after("value=")(Data_Show.showInt)(42 | 0);
export const applied = /* @__PURE__ */ after("partial=")(Data_Show.showInt);
export const dictionaryFirst = /* @__PURE__ */ before(Data_Show.showInt)("before=")(17 | 0);
export const local = /* @__PURE__ */ ((showADict) => (prefix) => (value) => /* @__PURE__ */ semigroupStringDictAppend(prefix)(/* @__PURE__ */ Data_Show.show(showADict)(value)))("local=")(Data_Show.showInt)(13 | 0);
