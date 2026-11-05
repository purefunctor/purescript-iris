import * as Control_Apply from "../Control.Apply/index.js";
import * as Control_Bind from "../Control.Bind/index.js";
import * as Data_Functor from "../Data.Functor/index.js";
import * as Data_Unit from "../Data.Unit/index.js";
import * as Effect from "../Effect/index.js";
import * as Effect_Exception from "../Effect.Exception/index.js";
import * as Iris_Effect_Async from "../Iris.Effect.Async/index.js";
import * as Iris_Effect_Fiber from "../Iris.Effect.Fiber/index.js";
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
function describe(section144) {
  if (section144.tag === "Succeeded") {
    return "succeeded";
  }
  if (section144.tag === "Failed") {
    const { _1: error } = section144;
    return "failed " + error;
  }
  if (section144.tag === "Died") {
    const { _1: defect } = section144;
    return "died " + Effect_Exception.message(defect);
  }
  throw new Error("Pattern match failure");
}
function outcome(report) {
  return (label) => {
    return (program$1) => {
      return /* @__PURE__ */ bindAsyncStringRowDictBind(Iris_Effect_Async.fork(program$1))((fiber) => /* @__PURE__ */ bindAsyncStringRowDictBind(Iris_Effect_Async.await(fiber))((exit) => Iris_Effect_Async.liftEffect(report(label + (": " + describe(exit))))));
    };
  };
}
function program(report) {
  const say = (message) => {
    return Iris_Effect_Async.liftEffect(report(message));
  };
  return /* @__PURE__ */ bindAsyncStringRowDictBind(Iris_Effect_Async.fork(Iris_Effect_Async.finally(say("released"))(Iris_Effect_Async.liftEffect(boom("boom")))))((dying) => /* @__PURE__ */ bindAsyncStringRowDictBind(Iris_Effect_Async.fork(/* @__PURE__ */ Control_Apply.applySecond(Iris_Effect_Async.applyAsync)(delay(10 | 0))(say("sibling completed"))))((other) => /* @__PURE__ */ bindAsyncStringRowDictBind(Iris_Effect_Async.await(other))(($exit) => /* @__PURE__ */ discardUnitDictDiscard(Iris_Effect_Async.bindAsync)(outcome(report)("await")(Iris_Effect_Async.join(dying)))(($unit) => /* @__PURE__ */ discardUnitDictDiscard(Iris_Effect_Async.bindAsync)(outcome(report)("attempt")(Iris_Effect_Async.attempt(Iris_Effect_Async.join(dying))))(($unit$1) => /* @__PURE__ */ discardUnitDictDiscard(Iris_Effect_Async.bindAsync)(outcome(report)("cleanup over typed")(Iris_Effect_Async.finally(Iris_Effect_Async.liftEffect(boom("cleanup")))(Iris_Effect_Async.fail("typed"))))(($unit$2) => /* @__PURE__ */ discardUnitDictDiscard(Iris_Effect_Async.bindAsync)(outcome(report)("body over cleanup")(Iris_Effect_Async.finally(Iris_Effect_Async.liftEffect(boom("cleanup")))(Iris_Effect_Async.liftEffect(boom("body")))))(($unit$3) => /* @__PURE__ */ bindAsyncStringRowDictBind(Iris_Effect_Async.fork(Iris_Effect_Async.finally(Iris_Effect_Async.liftEffect(boom("finalizer")))(delay(1e3 | 0))))((killed) => /* @__PURE__ */ discardUnitDictDiscard(Iris_Effect_Async.bindAsync)(Iris_Effect_Async.kill("killed")(killed))(($unit$4) => /* @__PURE__ */ bindAsyncStringRowDictBind(Iris_Effect_Async.await(killed))((killedExit) => /* @__PURE__ */ discardUnitDictDiscard(Iris_Effect_Async.bindAsync)(say("kill with failing finalizer: " + describe(killedExit)))(($unit$5) => /* @__PURE__ */ discardUnitDictDiscard(Iris_Effect_Async.bindAsync)(outcome(report)("null")(Iris_Effect_Async.liftEffect(throwNull)))(($unit$6) => /* @__PURE__ */ discardUnitDictDiscard(Iris_Effect_Async.bindAsync)(outcome(report)("hostile")(Iris_Effect_Async.liftEffect(throwHostile)))(($unit$7) => say("parent continues"))))))))))))));
}
export function run(report) {
  return /* @__PURE__ */ Data_Functor.void(Effect.functorEffect)(Iris_Effect_Async.launch({ onUncaught: (error) => report("uncaught " + error) })({})(program(report)));
}
export function launchDefect(report) {
  return /* @__PURE__ */ Data_Functor.void(Effect.functorEffect)(Iris_Effect_Async.launch({ onUncaught: (error) => report("uncaught " + error) })({})(Iris_Effect_Async.liftEffect(boom("root"))));
}
const boom = $foreign["boom"];
const throwNull = $foreign["throwNull"];
const throwHostile = $foreign["throwHostile"];
const setTimeout = $foreign["setTimeout"];
const clearTimeout = $foreign["clearTimeout"];
const bindAsyncStringRowDictBind = /* @__PURE__ */ Control_Bind.bind(Iris_Effect_Async.bindAsync);
const discardUnitDictDiscard = /* @__PURE__ */ Control_Bind.discard(Control_Bind.discardUnit);
