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
  return (label) => {
    return (milliseconds) => {
      return Iris_Effect_Async.async((callback) => {
        const $action = setTimeout(milliseconds)(callback({
          tag: "Right",
          _1: Data_Unit.unit
        }));
        return () => {
          const identifier = $action();
          const $value = /* @__PURE__ */ Control_Apply.applySecond(Iris_Effect_Async.applyAsync)(say(report)("cancel " + label))(Iris_Effect_Async.liftEffect(clearTimeout(identifier)));
          return $value;
        };
      });
    };
  };
}
function describe(section167) {
  if (section167.tag === "Succeeded") {
    return "succeeded";
  }
  if (section167.tag === "Failed") {
    const { _1: error } = section167;
    return "failed " + error;
  }
  if (section167.tag === "Died") {
    return "died";
  }
  throw new Error("Pattern match failure");
}
function killed(report) {
  return (label) => {
    return (body) => {
      return /* @__PURE__ */ bindAsyncStringRowDictBind(Iris_Effect_Async.fork(body))((fiber) => /* @__PURE__ */ discardUnitDictDiscard(Iris_Effect_Async.bindAsync)(Iris_Effect_Async.kill("kill " + label)(fiber))(($unit) => /* @__PURE__ */ bindAsyncStringRowDictBind(Iris_Effect_Async.await(fiber))((exit) => say(report)(label + (" " + describe(exit))))));
    };
  };
}
function program(report) {
  return /* @__PURE__ */ discardUnitDictDiscard(Iris_Effect_Async.bindAsync)(killed(report)("during acquire")(Iris_Effect_Async.bracket(/* @__PURE__ */ Control_Apply.applySecond(Iris_Effect_Async.applyAsync)(sleep(report)("acquire")(10 | 0))(say(report)("acquired")))(($unit) => say(report)("released"))(($unit$1) => say(report)("used"))))(($unit$2) => /* @__PURE__ */ discardUnitDictDiscard(Iris_Effect_Async.bindAsync)(killed(report)("during use")(Iris_Effect_Async.bracket(say(report)("acquired"))(($unit$3) => /* @__PURE__ */ Control_Apply.applySecond(Iris_Effect_Async.applyAsync)(sleep(report)("release")(10 | 0))(say(report)("released")))(($unit$4) => /* @__PURE__ */ Control_Apply.applySecond(Iris_Effect_Async.applyAsync)(sleep(report)("use")(1e3 | 0))(say(report)("used")))))(($unit$5) => killed(report)("with failing release")(Iris_Effect_Async.bracket(/* @__PURE__ */ Control_Applicative.pure(Iris_Effect_Async.applicativeAsync)(Data_Unit.unit))(($unit$6) => Iris_Effect_Async.fail("release failure"))(($unit$7) => sleep(report)("use")(1e3 | 0)))));
}
export function run(report) {
  return /* @__PURE__ */ Data_Functor.void(Effect.functorEffect)(Iris_Effect_Async.launch({ onUncaught: (error) => report("uncaught " + error) })({})(program(report)));
}
const setTimeout = $foreign["setTimeout"];
const clearTimeout = $foreign["clearTimeout"];
const bindAsyncStringRowDictBind = /* @__PURE__ */ Control_Bind.bind(Iris_Effect_Async.bindAsync);
const discardUnitDictDiscard = /* @__PURE__ */ Control_Bind.discard(Control_Bind.discardUnit);
