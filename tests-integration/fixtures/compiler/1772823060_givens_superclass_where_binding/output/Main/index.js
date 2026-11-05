import * as Control_Applicative from "../Control.Applicative/index.js";
import * as Control_Bind from "../Control.Bind/index.js";
import * as Data_Functor from "../Data.Functor/index.js";
export function test(monadRecMDict) {
  return (a) => ((x) => /* @__PURE__ */ Control_Applicative.pure(/* @__PURE__ */ (/* @__PURE__ */ monadRecMDict.Monad0()).Applicative0())(x))(a);
}
export function test2(monadRecMDict) {
  return (ma) => {
    return ((x) => {
      const Monad0Dict = /* @__PURE__ */ monadRecMDict.Monad0();
      return /* @__PURE__ */ Control_Bind.bind(/* @__PURE__ */ Monad0Dict.Bind1())(x)(/* @__PURE__ */ Control_Applicative.pure(/* @__PURE__ */ Monad0Dict.Applicative0()));
    })(ma);
  };
}
export function test3(monadRecMDict) {
  return (mi) => ((x) => /* @__PURE__ */ Data_Functor.map(/* @__PURE__ */ (/* @__PURE__ */ (/* @__PURE__ */ (/* @__PURE__ */ monadRecMDict.Monad0()).Applicative0()).Apply0()).Functor0())((y) => y)(x))(mi);
}
