import * as Control_Applicative from "../Control.Applicative/index.js";
import * as Control_Apply from "../Control.Apply/index.js";
import * as Control_Bind from "../Control.Bind/index.js";
import * as Data_Functor from "../Data.Functor/index.js";
import * as Data_Unit from "../Data.Unit/index.js";
import * as Iris_Effect_Async from "../Iris.Effect.Async/index.js";
import * as $foreign from "./foreign.js";
function throughMonad(monadMonadDict) {
  return (action) => /* @__PURE__ */ Control_Bind.bind(/* @__PURE__ */ monadMonadDict.Bind1())(action)((value) => /* @__PURE__ */ Control_Applicative.pure(/* @__PURE__ */ monadMonadDict.Applicative0())(value * (3 | 0) | 0));
}
function program(hasCounterServicesDict) {
  return (input) => /* @__PURE__ */ bindAsyncStringServicesDictBind(/* @__PURE__ */ Iris_Effect_Async.service(hasCounterServicesDict))((offset) => /* @__PURE__ */ bindAsyncStringServicesDictBind(Iris_Effect_Async.liftEffect(step("first")(input)))((first) => /* @__PURE__ */ bindAsyncStringServicesDictBind(/* @__PURE__ */ throughMonad(Iris_Effect_Async.monadAsync)(/* @__PURE__ */ applicativeAsyncStringServicesDictPure(first + offset | 0)))((adjusted) => /* @__PURE__ */ bindAsyncStringServicesDictBind(/* @__PURE__ */ Control_Apply.apply(Iris_Effect_Async.applyAsync)(/* @__PURE__ */ Data_Functor.map(Iris_Effect_Async.functorAsync)((left) => (right) => left - right | 0)(Iris_Effect_Async.liftEffect(step("left")(adjusted))))(Iris_Effect_Async.liftEffect(step("right")(offset))))((result) => result > (0 | 0) ? /* @__PURE__ */ applicativeAsyncStringServicesDictPure(result) : Iris_Effect_Async.fail("negative result")))));
}
export function runOrdered($unit) {
  const environment = mark("environment")({ choice: 7 | 0 });
  const stored = mark("program")({ action: /* @__PURE__ */ program({ getService: ($services) => $services.choice })(11 | 0) });
  const $action = Iris_Effect_Async.launch({ onUncaught: failure })(environment)(/* @__PURE__ */ bindAsyncStringServicesDictBind(stored.action)((result) => Iris_Effect_Async.liftEffect(complete(result))));
  return () => {
    const $fiber = $action();
    const $value = Data_Unit.unit;
    return $value;
  };
}
const step = $foreign["step"];
const complete = $foreign["complete"];
const failure = $foreign["failure"];
const mark = $foreign["mark"];
const bindAsyncStringServicesDictBind = /* @__PURE__ */ Control_Bind.bind(Iris_Effect_Async.bindAsync);
const applicativeAsyncStringServicesDictPure = /* @__PURE__ */ Control_Applicative.pure(Iris_Effect_Async.applicativeAsync);
export const provided = /* @__PURE__ */ (() => {
  const $action = Iris_Effect_Async.launch({ onUncaught: failure })({})(/* @__PURE__ */ bindAsyncStringServicesDictBind(Iris_Effect_Async.provide({ renamed: 3 | 0 })(/* @__PURE__ */ program({ getService: ($services) => $services.renamed })(5 | 0)))((result) => Iris_Effect_Async.liftEffect(complete(result))));
  return () => {
    const $fiber = $action();
    const $value = Data_Unit.unit;
    return $value;
  };
})();
