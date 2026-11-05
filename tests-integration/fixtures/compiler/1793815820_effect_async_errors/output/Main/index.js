import * as Control_Applicative from "../Control.Applicative/index.js";
import * as Control_Apply from "../Control.Apply/index.js";
import * as Control_Bind from "../Control.Bind/index.js";
import * as Data_Either from "../Data.Either/index.js";
import * as Data_Functor from "../Data.Functor/index.js";
import * as Data_Show from "../Data.Show/index.js";
import * as Effect from "../Effect/index.js";
import * as Iris_Effect_Async from "../Iris.Effect.Async/index.js";
function say(report) {
  return (message) => {
    return Iris_Effect_Async.liftEffect(report(message));
  };
}
function describe(section78) {
  if (section78.tag === "Left") {
    const { _1: error } = section78;
    return "left " + error;
  }
  if (section78.tag === "Right") {
    const { _1: value } = section78;
    return "right " + /* @__PURE__ */ showIntDictShow(value);
  }
  throw new Error("Pattern match failure");
}
function program(report) {
  return /* @__PURE__ */ bindAsyncCodeRowDictBind(Iris_Effect_Async.catch(/* @__PURE__ */ Control_Apply.applySecond(Iris_Effect_Async.applyAsync)(/* @__PURE__ */ Control_Apply.applySecond(Iris_Effect_Async.applyAsync)(/* @__PURE__ */ Control_Apply.applySecond(Iris_Effect_Async.applyAsync)(say(report)("before fail"))(Iris_Effect_Async.fail("boom")))(say(report)("after fail")))(/* @__PURE__ */ applicativeAsyncStringRowDictPure(0 | 0)))((error) => /* @__PURE__ */ discardUnitDictDiscard(Iris_Effect_Async.bindAsync)(say(report)("caught " + error))(($unit) => /* @__PURE__ */ applicativeAsyncStringRowDictPure(7 | 0))))((recovered) => /* @__PURE__ */ discardUnitDictDiscard(Iris_Effect_Async.bindAsync)(say(report)("recovered " + /* @__PURE__ */ showIntDictShow(recovered)))(($unit$1) => /* @__PURE__ */ bindAsyncCodeRowDictBind(Iris_Effect_Async.attempt(Iris_Effect_Async.mapError((error$1) => "mapped " + error$1)(Iris_Effect_Async.fail("inner"))))((mapped) => /* @__PURE__ */ discardUnitDictDiscard(Iris_Effect_Async.bindAsync)(say(report)(describe(mapped)))(($unit$2) => /* @__PURE__ */ bindAsyncCodeRowDictBind(Iris_Effect_Async.attempt(Iris_Effect_Async.catch(Iris_Effect_Async.fail("first"))((error$2) => Iris_Effect_Async.fail("handled " + error$2))))((rethrown) => /* @__PURE__ */ discardUnitDictDiscard(Iris_Effect_Async.bindAsync)(say(report)(describe(rethrown)))(($unit$3) => /* @__PURE__ */ bindAsyncCodeRowDictBind(Iris_Effect_Async.attempt(/* @__PURE__ */ applicativeAsyncStringRowDictPure(3 | 0)))((succeeded) => /* @__PURE__ */ discardUnitDictDiscard(Iris_Effect_Async.bindAsync)(say(report)(describe(succeeded)))(($unit$4) => /* @__PURE__ */ discardUnitDictDiscard(Iris_Effect_Async.bindAsync)(Iris_Effect_Async.mapError((value) => value)(Iris_Effect_Async.fail(42 | 0)))(($unit$5) => say(report)("after uncaught"))))))))));
}
export function run(report) {
  return /* @__PURE__ */ Data_Functor.void(Effect.functorEffect)(Iris_Effect_Async.launch({ onUncaught: (code) => report("uncaught " + /* @__PURE__ */ showIntDictShow(code)) })({})(program(report)));
}
const showIntDictShow = /* @__PURE__ */ Data_Show.show(Data_Show.showInt);
const bindAsyncCodeRowDictBind = /* @__PURE__ */ Control_Bind.bind(Iris_Effect_Async.bindAsync);
const applicativeAsyncStringRowDictPure = /* @__PURE__ */ Control_Applicative.pure(Iris_Effect_Async.applicativeAsync);
const discardUnitDictDiscard = /* @__PURE__ */ Control_Bind.discard(Control_Bind.discardUnit);
