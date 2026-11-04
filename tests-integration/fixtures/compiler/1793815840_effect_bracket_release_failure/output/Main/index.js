import * as Control_Applicative from "../Control.Applicative/index.js";
import * as Control_Bind from "../Control.Bind/index.js";
import * as Data_Either from "../Data.Either/index.js";
import * as Data_Functor from "../Data.Functor/index.js";
import * as Data_Show from "../Data.Show/index.js";
import * as Data_Unit from "../Data.Unit/index.js";
import * as Effect from "../Effect/index.js";
import * as Iris_Effect_Async from "../Iris.Effect.Async/index.js";
function describe(section48) {
  if (section48.tag === "Left") {
    const { _1: error } = section48;
    return "failed " + error;
  }
  if (section48.tag === "Right") {
    const { _1: value } = section48;
    return "succeeded " + /* @__PURE__ */ Data_Show.show(Data_Show.showInt)(value);
  }
  throw new Error("Pattern match failure");
}
function guarded(release) {
  return (body) => {
    return Iris_Effect_Async.attempt(Iris_Effect_Async.bracket(/* @__PURE__ */ applicativeAsyncStringRowDictPure(Data_Unit.unit))(($unit) => release)(($unit$1) => body));
  };
}
function program(report) {
  const report_ = (message) => {
    return Iris_Effect_Async.liftEffect(report(message));
  };
  return /* @__PURE__ */ bindAsyncStringRowDictBind(guarded(Iris_Effect_Async.fail("release"))(/* @__PURE__ */ applicativeAsyncStringRowDictPure(1 | 0)))((releaseFailed) => /* @__PURE__ */ discardUnitDictDiscard(Iris_Effect_Async.bindAsync)(report_("release fails: " + describe(releaseFailed)))(($unit) => /* @__PURE__ */ bindAsyncStringRowDictBind(guarded(/* @__PURE__ */ applicativeAsyncStringRowDictPure(Data_Unit.unit))(Iris_Effect_Async.fail("body")))((bodyFailed) => /* @__PURE__ */ discardUnitDictDiscard(Iris_Effect_Async.bindAsync)(report_("body fails: " + describe(bodyFailed)))(($unit$1) => /* @__PURE__ */ bindAsyncStringRowDictBind(guarded(Iris_Effect_Async.fail("release"))(Iris_Effect_Async.fail("body")))((bothFailed) => /* @__PURE__ */ discardUnitDictDiscard(Iris_Effect_Async.bindAsync)(report_("both fail: " + describe(bothFailed)))(($unit$2) => /* @__PURE__ */ bindAsyncStringRowDictBind(guarded(/* @__PURE__ */ applicativeAsyncStringRowDictPure(Data_Unit.unit))(/* @__PURE__ */ applicativeAsyncStringRowDictPure(2 | 0)))((neitherFailed) => report_("neither fails: " + describe(neitherFailed)))))))));
}
export function run(report) {
  return /* @__PURE__ */ Data_Functor.void(Effect.functorEffect)(Iris_Effect_Async.launch({ onUncaught: (error) => report("uncaught " + error) })({})(program(report)));
}
const applicativeAsyncStringRowDictPure = /* @__PURE__ */ Control_Applicative.pure(Iris_Effect_Async.applicativeAsync);
const bindAsyncStringRowDictBind = /* @__PURE__ */ Control_Bind.bind(Iris_Effect_Async.bindAsync);
const discardUnitDictDiscard = /* @__PURE__ */ Control_Bind.discard(Control_Bind.discardUnit);
