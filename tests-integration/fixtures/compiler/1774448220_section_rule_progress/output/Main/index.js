import * as Control_Applicative from "../Control.Applicative/index.js";
import * as Control_Apply from "../Control.Apply/index.js";
import * as Control_Bind from "../Control.Bind/index.js";
import * as Data_Functor from "../Data.Functor/index.js";
export const Done = ($value0) => ({
  tag: "Done",
  _1: $value0
});
export const Loop = ($value0) => ({
  tag: "Loop",
  _1: $value0
});
export function append(dictionary) {
  return dictionary.append;
}
export function mempty(dictionary) {
  return dictionary.mempty;
}
export function tailRecM(dictionary) {
  return dictionary.tailRecM;
}
export function ifM(bindMDict) {
  return (mb) => (t) => (f) => /* @__PURE__ */ Control_Bind.bind(bindMDict)(mb)((b) => b ? t : f);
}
export function lift2(applyFDict) {
  return (f) => (fa) => (fb) => /* @__PURE__ */ Control_Apply.apply(applyFDict)(/* @__PURE__ */ Data_Functor.map(/* @__PURE__ */ applyFDict.Functor0())(f)(fa))(fb);
}
export function liftIfM(applyFDict) {
  return (bindMDict) => {
    return (p) => (x) => (y) => /* @__PURE__ */ lift2(applyFDict)(/* @__PURE__ */ ifM(bindMDict)(p))(x)(y);
  };
}
export function appendM(monadRecMDict) {
  return (applicativeFDict) => {
    return (semigroupFADict) => {
      return (xs) => (f) => /* @__PURE__ */ Control_Bind.bind(/* @__PURE__ */ monadRecMDict.Bind1())(f)((x) => /* @__PURE__ */ loop(/* @__PURE__ */ monadRecMDict.Applicative0())(/* @__PURE__ */ append(semigroupFADict)(xs)(/* @__PURE__ */ Control_Applicative.pure(applicativeFDict)(x))));
    };
  };
}
export function loop(applicativeMDict) {
  return (a) => /* @__PURE__ */ Control_Applicative.pure(applicativeMDict)({
    tag: "Loop",
    _1: a
  });
}
export function done(applicativeMDict) {
  return (b) => /* @__PURE__ */ Control_Applicative.pure(applicativeMDict)({
    tag: "Done",
    _1: b
  });
}
function whileM_(monadRecMDict) {
  return (applicativeFDict) => {
    return (monoidFADict) => {
      return (p) => (f) => /* @__PURE__ */ tailRecM(monadRecMDict)(/* @__PURE__ */ liftIfM(Control_Apply.applyFn)(/* @__PURE__ */ monadRecMDict.Bind1())(p)((section417) => /* @__PURE__ */ appendM(monadRecMDict)(applicativeFDict)(/* @__PURE__ */ monoidFADict.Semigroup0())(section417)(f))(/* @__PURE__ */ done(/* @__PURE__ */ monadRecMDict.Applicative0())))(/* @__PURE__ */ mempty(monoidFADict));
    };
  };
}
export { whileM_ as "whileM'" };
