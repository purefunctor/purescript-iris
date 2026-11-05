import * as Control_Applicative from "../Control.Applicative/index.js";
import * as Control_Apply from "../Control.Apply/index.js";
import * as Control_Bind from "../Control.Bind/index.js";
import * as Data_Functor from "../Data.Functor/index.js";
import * as $runtime from "../runtime.js";
export const Box = ($value0) => ({
  tag: "Box",
  _1: $value0
});
export function liftApplicative(applicativeFunctorDict) {
  return ($function) => (value) => /* @__PURE__ */ Control_Apply.apply(/* @__PURE__ */ applicativeFunctorDict.Apply0())(/* @__PURE__ */ Control_Applicative.pure(applicativeFunctorDict)($function))(value);
}
export function applyMonad(monadMonadDict) {
  return (functions) => (values) => /* @__PURE__ */ Control_Bind.bind(/* @__PURE__ */ monadMonadDict.Bind1())(functions)(($function) => /* @__PURE__ */ Control_Bind.bind(/* @__PURE__ */ monadMonadDict.Bind1())(values)((value) => /* @__PURE__ */ Control_Applicative.pure(/* @__PURE__ */ monadMonadDict.Applicative0())($function(value))));
}
const $lazy_functorBox = $runtime.binding("functorBox", () => {
  return { map: /* @__PURE__ */ liftApplicative($lazy_applicativeBox()) };
});
const $lazy_applyBox = $runtime.binding("applyBox", () => {
  return {
    Functor0: () => $lazy_functorBox(),
    apply: /* @__PURE__ */ applyMonad($lazy_monadBox())
  };
});
const $lazy_applicativeBox = $runtime.binding("applicativeBox", () => {
  return {
    Apply0: () => $lazy_applyBox(),
    pure: Box
  };
});
const $lazy_bindBox = $runtime.binding("bindBox", () => {
  return {
    Apply0: () => $lazy_applyBox(),
    bind: ($box) => {
      if ($box.tag === "Box") {
        const { _1: value } = $box;
        return (continuation) => {
          return continuation(value);
        };
      } else {
        throw new Error("Pattern match failure");
      }
    }
  };
});
const $lazy_monadBox = $runtime.binding("monadBox", () => {
  return {
    Applicative0: () => $lazy_applicativeBox(),
    Bind1: () => $lazy_bindBox()
  };
});
export const functorBox = $lazy_functorBox();
export const applyBox = $lazy_applyBox();
export const applicativeBox = $lazy_applicativeBox();
export const bindBox = $lazy_bindBox();
export const monadBox = $lazy_monadBox();
export const result = (() => {
  const $scrutinee = /* @__PURE__ */ Data_Functor.map($lazy_functorBox())((value) => value)({
    tag: "Box",
    _1: 42 | 0
  });
  if ($scrutinee.tag === "Box") {
    const { _1: value$1 } = $scrutinee;
    return value$1;
  }
  throw new Error("Pattern match failure");
})();
