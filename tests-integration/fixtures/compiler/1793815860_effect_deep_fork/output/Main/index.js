import * as Control_Applicative from "../Control.Applicative/index.js";
import * as Control_Bind from "../Control.Bind/index.js";
import * as Data_Functor from "../Data.Functor/index.js";
import * as Data_Show from "../Data.Show/index.js";
import * as Effect from "../Effect/index.js";
import * as Iris_Effect_Async from "../Iris.Effect.Async/index.js";
function nestedForks(depth) {
  return /* @__PURE__ */ bindAsyncStringRowDictBind(/* @__PURE__ */ applicativeAsyncStringRowDictPure(depth))((value) => value === (0 | 0) ? /* @__PURE__ */ applicativeAsyncStringRowDictPure(719 | 0) : /* @__PURE__ */ bindAsyncStringRowDictBind(Iris_Effect_Async.fork(nestedForks(value - (1 | 0) | 0)))((child) => Iris_Effect_Async.join(child)));
}
function countdown(remaining) {
  if (remaining === (0 | 0)) {
    return /* @__PURE__ */ applicativeAsyncStringRowDictPure(0 | 0);
  } else {
    return /* @__PURE__ */ bindAsyncStringRowDictBind(/* @__PURE__ */ applicativeAsyncStringRowDictPure(remaining))((value) => /* @__PURE__ */ bindAsyncStringRowDictBind(countdown(value - (1 | 0) | 0))((next) => /* @__PURE__ */ applicativeAsyncStringRowDictPure(next + (1 | 0) | 0)));
  }
}
function program(report) {
  return /* @__PURE__ */ bindAsyncStringRowDictBind(nestedForks(1e4 | 0))((forked) => /* @__PURE__ */ Control_Bind.discard(Control_Bind.discardUnit)(Iris_Effect_Async.bindAsync)(Iris_Effect_Async.liftEffect(report("nested forks " + /* @__PURE__ */ showIntDictShow(forked))))(($unit) => /* @__PURE__ */ bindAsyncStringRowDictBind(countdown(1e5 | 0))((counted) => Iris_Effect_Async.liftEffect(report("binds " + /* @__PURE__ */ showIntDictShow(counted))))));
}
export function run(report) {
  return /* @__PURE__ */ Data_Functor.void(Effect.functorEffect)(Iris_Effect_Async.launch({ onUncaught: (error) => report("uncaught " + error) })({})(program(report)));
}
const bindAsyncStringRowDictBind = /* @__PURE__ */ Control_Bind.bind(Iris_Effect_Async.bindAsync);
const applicativeAsyncStringRowDictPure = /* @__PURE__ */ Control_Applicative.pure(Iris_Effect_Async.applicativeAsync);
const showIntDictShow = /* @__PURE__ */ Data_Show.show(Data_Show.showInt);
