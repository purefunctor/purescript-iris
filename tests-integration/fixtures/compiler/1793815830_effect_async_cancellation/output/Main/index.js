import * as Control_Applicative from "../Control.Applicative/index.js";
import * as Control_Apply from "../Control.Apply/index.js";
import * as Control_Bind from "../Control.Bind/index.js";
import * as Data_Functor from "../Data.Functor/index.js";
import * as Data_Unit from "../Data.Unit/index.js";
import * as Effect from "../Effect/index.js";
import * as Iris_Effect_Async from "../Iris.Effect.Async/index.js";
import * as Iris_Effect_Fiber from "../Iris.Effect.Fiber/index.js";
import * as $foreign from "./foreign.js";
function say(report) {
  return (message) => {
    return Iris_Effect_Async.liftEffect(report(message));
  };
}
function sleep(report) {
  return (milliseconds) => {
    return Iris_Effect_Async.async((callback) => {
      const $action = setTimeout(milliseconds)(callback({
        tag: "Right",
        _1: Data_Unit.unit
      }));
      return () => {
        const identifier = $action();
        const $value = /* @__PURE__ */ Control_Apply.applySecond(Iris_Effect_Async.applyAsync)(say(report)("canceler"))(Iris_Effect_Async.liftEffect(clearTimeout(identifier)));
        return $value;
      };
    });
  };
}
function describe(section159) {
  if (section159.tag === "Succeeded") {
    return "succeeded";
  }
  if (section159.tag === "Failed") {
    const { _1: error } = section159;
    return "failed " + error;
  }
  if (section159.tag === "Died") {
    return "died";
  }
  throw new Error("Pattern match failure");
}
function program(report) {
  return /* @__PURE__ */ bindAsyncStringRowDictBind(Iris_Effect_Async.fork(/* @__PURE__ */ Control_Apply.applySecond(Iris_Effect_Async.applyAsync)(sleep(report)(1e3 | 0))(say(report)("woke"))))((sleeping) => /* @__PURE__ */ discardUnitDictDiscard(Iris_Effect_Async.bindAsync)(say(report)("forked"))(($unit) => /* @__PURE__ */ discardUnitDictDiscard(Iris_Effect_Async.bindAsync)(Iris_Effect_Async.kill("stop")(sleeping))(($unit$1) => /* @__PURE__ */ discardUnitDictDiscard(Iris_Effect_Async.bindAsync)(say(report)("killed"))(($unit$2) => /* @__PURE__ */ bindAsyncStringRowDictBind(Iris_Effect_Async.await(sleeping))((sleepingExit) => /* @__PURE__ */ discardUnitDictDiscard(Iris_Effect_Async.bindAsync)(say(report)("sleeping " + describe(sleepingExit)))(($unit$3) => /* @__PURE__ */ bindAsyncStringRowDictBind(Iris_Effect_Async.fork(/* @__PURE__ */ Control_Applicative.pure(Iris_Effect_Async.applicativeAsync)(Data_Unit.unit)))((finished) => /* @__PURE__ */ bindAsyncStringRowDictBind(Iris_Effect_Async.await(finished))(($exit) => /* @__PURE__ */ discardUnitDictDiscard(Iris_Effect_Async.bindAsync)(Iris_Effect_Async.kill("too late")(finished))(($unit$4) => /* @__PURE__ */ bindAsyncStringRowDictBind(Iris_Effect_Async.await(finished))((finishedExit) => /* @__PURE__ */ discardUnitDictDiscard(Iris_Effect_Async.bindAsync)(say(report)("finished " + describe(finishedExit)))(($unit$5) => /* @__PURE__ */ discardUnitDictDiscard(Iris_Effect_Async.bindAsync)(sleep(report)(20 | 0))(($unit$6) => say(report)("done")))))))))))));
}
export function run(report) {
  return /* @__PURE__ */ Data_Functor.void(Effect.functorEffect)(Iris_Effect_Async.launch({ onUncaught: (error) => report("uncaught " + error) })({})(program(report)));
}
const setTimeout = $foreign["setTimeout"];
const clearTimeout = $foreign["clearTimeout"];
const bindAsyncStringRowDictBind = /* @__PURE__ */ Control_Bind.bind(Iris_Effect_Async.bindAsync);
const discardUnitDictDiscard = /* @__PURE__ */ Control_Bind.discard(Control_Bind.discardUnit);
