import * as Control_Applicative from "../Control.Applicative/index.js";
import * as Control_Apply from "../Control.Apply/index.js";
import * as Control_Bind from "../Control.Bind/index.js";
import * as Data_Either from "../Data.Either/index.js";
import * as Data_Functor from "../Data.Functor/index.js";
import * as Data_Show from "../Data.Show/index.js";
import * as Data_Unit from "../Data.Unit/index.js";
import * as Effect from "../Effect/index.js";
import * as Iris_Effect_Async from "../Iris.Effect.Async/index.js";
import * as $foreign from "./foreign.js";
function delay(milliseconds) {
  return Iris_Effect_Async.async((callback) => {
    const $action = setTimeout(milliseconds)(callback({
      tag: "Right",
      _1: Data_Unit.unit
    }));
    return () => {
      const identifier = $action();
      const $value = Iris_Effect_Async.liftEffect(clearTimeout(identifier));
      return $value;
    };
  });
}
function describe(section121) {
  if (section121.tag === "Left") {
    const { _1: error } = section121;
    return "failed " + error;
  }
  if (section121.tag === "Right") {
    const { _1: value } = section121;
    return "succeeded " + /* @__PURE__ */ showIntDictShow(value);
  }
  throw new Error("Pattern match failure");
}
function describeOuter(section160) {
  if (section160.tag === "Left") {
    const { _1: code } = section160;
    return "failed outer " + /* @__PURE__ */ showIntDictShow(code);
  }
  if (section160.tag === "Right") {
    const { _1: value } = section160;
    return "succeeded " + /* @__PURE__ */ showIntDictShow(value);
  }
  throw new Error("Pattern match failure");
}
function children(report) {
  return /* @__PURE__ */ bindAsyncStringRowDictBind(Iris_Effect_Async.fork(Iris_Effect_Async.fail("detached child failure")))(($fiber) => /* @__PURE__ */ bindAsyncStringRowDictBind(Iris_Effect_Async.fork(/* @__PURE__ */ Control_Apply.applySecond(Iris_Effect_Async.applyAsync)(delay(10 | 0))(Iris_Effect_Async.fail("joined child failure"))))((failing) => /* @__PURE__ */ bindAsyncStringRowDictBind(Iris_Effect_Async.fork(/* @__PURE__ */ Control_Apply.applySecond(Iris_Effect_Async.applyAsync)(delay(10 | 0))(/* @__PURE__ */ applicativeAsyncStringRowDictPure(5 | 0))))((succeeding) => /* @__PURE__ */ bindAsyncStringRowDictBind(Iris_Effect_Async.fork(innerRelease))((guarded) => /* @__PURE__ */ discardUnitDictDiscard(Iris_Effect_Async.bindAsync)(delay(20 | 0))(($unit) => /* @__PURE__ */ discardUnitDictDiscard(Iris_Effect_Async.bindAsync)(Iris_Effect_Async.kill("killed")(guarded))(($unit$1) => /* @__PURE__ */ bindAsyncStringRowDictBind(Iris_Effect_Async.attempt(Iris_Effect_Async.join(failing)))((joinedFailing) => /* @__PURE__ */ bindAsyncStringRowDictBind(Iris_Effect_Async.attempt(Iris_Effect_Async.join(succeeding)))((joinedSucceeding) => /* @__PURE__ */ bindAsyncStringRowDictBind(Iris_Effect_Async.attempt(Iris_Effect_Async.join(guarded)))((joinedGuarded) => /* @__PURE__ */ discardUnitDictDiscard(Iris_Effect_Async.bindAsync)(Iris_Effect_Async.liftEffect(report(describe(joinedFailing) + (", " + (describe(joinedSucceeding) + (", " + describe(joinedGuarded)))))))(($unit$2) => Iris_Effect_Async.fail("parent failure")))))))))));
}
function program(report) {
  return /* @__PURE__ */ bindAsyncStringRowDictBind(Iris_Effect_Async.fork(Iris_Effect_Async.catch(innerRelease)(($string) => Iris_Effect_Async.fail(1 | 0))))((mapped) => /* @__PURE__ */ bindAsyncStringRowDictBind(Iris_Effect_Async.fork(Iris_Effect_Async.catch(innerCanceler)(($string$1) => Iris_Effect_Async.fail(2 | 0))))((cancelled) => /* @__PURE__ */ discardUnitDictDiscard(Iris_Effect_Async.bindAsync)(delay(20 | 0))(($unit) => /* @__PURE__ */ discardUnitDictDiscard(Iris_Effect_Async.bindAsync)(Iris_Effect_Async.kill(7 | 0)(mapped))(($unit$1) => /* @__PURE__ */ discardUnitDictDiscard(Iris_Effect_Async.bindAsync)(Iris_Effect_Async.kill(8 | 0)(cancelled))(($unit$2) => /* @__PURE__ */ bindAsyncStringRowDictBind(Iris_Effect_Async.attempt(Iris_Effect_Async.join(mapped)))((joinedMapped) => /* @__PURE__ */ bindAsyncStringRowDictBind(Iris_Effect_Async.attempt(Iris_Effect_Async.join(cancelled)))((joinedCancelled) => /* @__PURE__ */ discardUnitDictDiscard(Iris_Effect_Async.bindAsync)(Iris_Effect_Async.liftEffect(report(describeOuter(joinedMapped) + (", " + describeOuter(joinedCancelled)))))(($unit$3) => /* @__PURE__ */ bindAsyncStringRowDictBind(Iris_Effect_Async.catch(children(report))((error) => /* @__PURE__ */ discardUnitDictDiscard(Iris_Effect_Async.bindAsync)(Iris_Effect_Async.liftEffect(report("caught " + error)))(($unit$4) => Iris_Effect_Async.fail(99 | 0))))(($int) => /* @__PURE__ */ applicativeAsyncStringRowDictPure(Data_Unit.unit))))))))));
}
export function run(report) {
  return /* @__PURE__ */ Data_Functor.void(Effect.functorEffect)(Iris_Effect_Async.launch({ onUncaught: (code) => report("uncaught " + /* @__PURE__ */ showIntDictShow(code)) })({})(program(report)));
}
const setTimeout = $foreign["setTimeout"];
const clearTimeout = $foreign["clearTimeout"];
const showIntDictShow = /* @__PURE__ */ Data_Show.show(Data_Show.showInt);
const applicativeAsyncStringRowDictPure = /* @__PURE__ */ Control_Applicative.pure(Iris_Effect_Async.applicativeAsync);
const bindAsyncStringRowDictBind = /* @__PURE__ */ Control_Bind.bind(Iris_Effect_Async.bindAsync);
const discardUnitDictDiscard = /* @__PURE__ */ Control_Bind.discard(Control_Bind.discardUnit);
const innerRelease = Iris_Effect_Async.bracket(/* @__PURE__ */ applicativeAsyncStringRowDictPure(Data_Unit.unit))(($unit) => Iris_Effect_Async.fail("release"))(($unit$1) => /* @__PURE__ */ Control_Apply.applySecond(Iris_Effect_Async.applyAsync)(delay(1e3 | 0))(/* @__PURE__ */ applicativeAsyncStringRowDictPure(9 | 0)));
const innerCanceler = /* @__PURE__ */ (() => {
  return Iris_Effect_Async.async(($function) => {
    const $value = Iris_Effect_Async.fail("cleanup");
    return () => {
      return $value;
    };
  });
})();
